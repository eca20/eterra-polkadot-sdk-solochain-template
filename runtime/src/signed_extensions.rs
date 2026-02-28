use alloc::vec;
use codec::{Decode, Encode};
use frame_support::dispatch::DispatchInfo;
use scale_info::TypeInfo;
use sp_runtime::{
    traits::{DispatchInfoOf, Dispatchable, One, SignedExtension, Zero},
    transaction_validity::{
        InvalidTransaction, TransactionLongevity, TransactionValidity, TransactionValidityError,
        ValidTransaction,
    },
    DispatchResult,
};

use crate::{AccountId, Nonce, Runtime, RuntimeCall};

/// Check nonce and increment to give replay protection for transactions, while allowing a
/// brand-new account (providers == 0 and sufficients == 0) to submit a *single* signed faucet
/// claim to itself so it can get initial funds.
///
/// Background: `frame_system::CheckNonce` rejects accounts with both providers and sufficients
/// equal to zero (nonce storage "not paid for"). That blocks a new user from submitting a signed
/// faucet claim, even when the runtime sponsors fees.
#[derive(Encode, Decode, Clone, Eq, PartialEq, TypeInfo, Debug)]
pub struct CheckNonceWithFaucet(#[codec(compact)] pub Nonce);

impl CheckNonceWithFaucet {
    /// Utility constructor. Used in client/factory code.
    pub fn from(nonce: Nonce) -> Self {
        Self(nonce)
    }
}

#[derive(Clone)]
pub enum Pre {
    None,
    PostDispatchIncrement { who: AccountId, nonce: Nonce },
}

fn is_self_faucet_claim(call: &RuntimeCall, who: &AccountId) -> bool {
    matches!(
        call,
        RuntimeCall::EterraFaucet(pallet_eterra_faucet::Call::claim { dest }) if dest == who
    )
}

impl SignedExtension for CheckNonceWithFaucet
where
    RuntimeCall: Dispatchable<Info = DispatchInfo>,
{
    // Keep the identifier as `CheckNonce` so existing tooling (polkadot-js, PAPI, etc.) treats
    // this as the standard nonce extension when constructing and signing extrinsics.
    const IDENTIFIER: &'static str = "CheckNonce";

    type AccountId = AccountId;
    type Call = RuntimeCall;
    type AdditionalSigned = ();
    type Pre = Pre;

    fn additional_signed(&self) -> Result<(), TransactionValidityError> {
        Ok(())
    }

    fn validate(
        &self,
        who: &Self::AccountId,
        call: &Self::Call,
        _info: &DispatchInfoOf<Self::Call>,
        _len: usize,
    ) -> TransactionValidity {
        let account = frame_system::Account::<Runtime>::get(who);

        if account.providers.is_zero() && account.sufficients.is_zero() {
            // Brand-new accounts are only allowed to submit a self-faucet claim.
            if !is_self_faucet_claim(call, who) {
                return InvalidTransaction::Payment.into();
            }

            // For non-existent accounts we disallow queued future nonces; the first tx must use
            // the currently stored nonce (normally 0).
            if self.0 != account.nonce {
                return Err(if self.0 < account.nonce {
                    InvalidTransaction::Stale
                } else {
                    InvalidTransaction::Future
                }
                .into());
            }

            let provides = vec![Encode::encode(&(who, self.0))];
            return Ok(ValidTransaction {
                priority: 0,
                requires: vec![],
                provides,
                longevity: TransactionLongevity::max_value(),
                propagate: true,
            });
        }

        // Normal CheckNonce behavior for existing accounts.
        if self.0 < account.nonce {
            return InvalidTransaction::Stale.into();
        }

        let provides = vec![Encode::encode(&(who, self.0))];
        let requires = if account.nonce < self.0 {
            vec![Encode::encode(&(who, self.0.saturating_sub(1)))]
        } else {
            vec![]
        };

        Ok(ValidTransaction {
            priority: 0,
            requires,
            provides,
            longevity: TransactionLongevity::max_value(),
            propagate: true,
        })
    }

    fn pre_dispatch(
        self,
        who: &Self::AccountId,
        call: &Self::Call,
        _info: &DispatchInfoOf<Self::Call>,
        _len: usize,
    ) -> Result<Self::Pre, TransactionValidityError> {
        let mut account = frame_system::Account::<Runtime>::get(who);

        if account.providers.is_zero() && account.sufficients.is_zero() {
            if !is_self_faucet_claim(call, who) {
                // Nonce storage not paid for
                return Err(InvalidTransaction::Payment.into());
            }

            if self.0 != account.nonce {
                return Err(if self.0 < account.nonce {
                    InvalidTransaction::Stale
                } else {
                    InvalidTransaction::Future
                }
                .into());
            }

            // Don't write nonce storage for non-existent accounts up-front; wait until the call
            // succeeds and (typically) creates the account via the faucet transfer.
            return Ok(Pre::PostDispatchIncrement {
                who: who.clone(),
                nonce: self.0,
            });
        }

        if self.0 != account.nonce {
            return Err(if self.0 < account.nonce {
                InvalidTransaction::Stale
            } else {
                InvalidTransaction::Future
            }
            .into());
        }

        account.nonce += Nonce::one();
        frame_system::Account::<Runtime>::insert(who, account);
        Ok(Pre::None)
    }

    fn post_dispatch(
        pre: Option<Self::Pre>,
        _info: &DispatchInfoOf<Self::Call>,
        _post_info: &sp_runtime::traits::PostDispatchInfoOf<Self::Call>,
        _len: usize,
        result: &DispatchResult,
    ) -> Result<(), TransactionValidityError> {
        let Some(pre) = pre else {
            return Ok(());
        };

        if let Pre::PostDispatchIncrement { who, nonce } = pre {
            if result.is_ok() {
                frame_system::Account::<Runtime>::mutate(&who, |acct| {
                    // Preserve providers/sufficients/data set by the faucet transfer.
                    acct.nonce = nonce.saturating_add(1);
                });
            }
        }

        Ok(())
    }
}
