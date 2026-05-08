use super::*;

use codec::Encode;
use sp_core::{sr25519, Pair};
use sp_io::TestExternalities;
use sp_runtime::{
    generic::Era,
    traits::Header as _,
    transaction_validity::{InvalidTransaction, TransactionSource, TransactionValidityError},
    BuildStorage,
};

fn account_id_from_pair(pair: &sr25519::Pair) -> AccountId {
    sp_runtime::AccountId32::from(pair.public()).into()
}

fn new_test_ext_with_faucet(faucet: &AccountId, faucet_balance: Balance) -> TestExternalities {
    let genesis = RuntimeGenesisConfig {
        balances: pallet_balances::GenesisConfig {
            balances: vec![(faucet.clone(), faucet_balance)],
        },
        eterra_faucet: pallet_eterra_faucet::GenesisConfig {
            faucet_account: Some(faucet.clone()),
            payout_amount: 1_000 * UNIT,
        },
        ..Default::default()
    };

    TestExternalities::new(
        genesis
            .build_storage()
            .expect("runtime genesis storage should build"),
    )
}

fn signed_extrinsic(
    call: RuntimeCall,
    signer: &sr25519::Pair,
    nonce: Nonce,
    genesis_hash: Hash,
    best_hash: Hash,
) -> UncheckedExtrinsic {
    // Keep this in sync with `node/src/benchmarking.rs` so we exercise the same SignedExtra set
    // that the node uses in production.
    let extra: SignedExtra = (
        frame_system::CheckNonZeroSender::<Runtime>::new(),
        frame_system::CheckSpecVersion::<Runtime>::new(),
        frame_system::CheckTxVersion::<Runtime>::new(),
        frame_system::CheckGenesis::<Runtime>::new(),
        frame_system::CheckEra::<Runtime>::from(Era::Immortal),
        CheckNonceWithFaucet::from(nonce),
        frame_system::CheckWeight::<Runtime>::new(),
        pallet_transaction_payment::ChargeTransactionPayment::<Runtime>::from(0),
        frame_metadata_hash_extension::CheckMetadataHash::<Runtime>::new(false),
    );

    let raw_payload = SignedPayload::from_raw(
        call.clone(),
        extra.clone(),
        (
            (),
            VERSION.spec_version,
            VERSION.transaction_version,
            genesis_hash,
            best_hash,
            (),
            (),
            (),
            None,
        ),
    );

    let signature = raw_payload.using_encoded(|e| signer.sign(e));

    UncheckedExtrinsic::new_signed(
        call,
        sp_runtime::AccountId32::from(signer.public()).into(),
        Signature::Sr25519(signature),
        extra,
    )
}

#[test]
fn faucet_claim_from_zero_balance_is_valid_transaction() {
    let faucet_pair = sr25519::Pair::from_string("//Alice", None).expect("dev key should parse");
    let faucet = account_id_from_pair(&faucet_pair);

    let claimant_pair =
        sr25519::Pair::from_string("//Charlie", None).expect("dev key should parse");
    let claimant = account_id_from_pair(&claimant_pair);

    new_test_ext_with_faucet(&faucet, 1u128 << 60).execute_with(|| {
        System::set_block_number(1);
        let genesis_hash = System::block_hash(0);

        // Sanity: this account truly has no funds in our genesis.
        assert_eq!(Balances::free_balance(&claimant), 0);

        let call = RuntimeCall::EterraFaucet(pallet_eterra_faucet::Call::claim {
            dest: claimant.clone(),
        });
        let xt = signed_extrinsic(call, &claimant_pair, 0, genesis_hash, genesis_hash);

        // This mirrors what the node does when a transaction is submitted to the pool.
        let validity =
            Executive::validate_transaction(TransactionSource::External, xt.clone(), genesis_hash);
        assert!(
            validity.is_ok(),
            "expected faucet claim from zero-balance account to be valid, got: {validity:?}"
        );

        // Apply the extrinsic and ensure nonce is incremented (replay protection) and funds land.
        let header = Header::new(
            1,
            Default::default(),
            Default::default(),
            genesis_hash,
            Default::default(),
        );
        Executive::initialize_block(&header);

        let dispatch_outcome =
            Executive::apply_extrinsic(xt).expect("faucet claim should pass signed extensions");
        assert!(
            dispatch_outcome.is_ok(),
            "faucet claim dispatch failed unexpectedly: {dispatch_outcome:?}"
        );

        assert_eq!(Balances::free_balance(&claimant), 1_000 * UNIT);
        assert_eq!(System::account(&claimant).nonce, 1);
    });
}

#[test]
fn non_faucet_call_from_zero_balance_is_rejected_for_payment() {
    let faucet_pair = sr25519::Pair::from_string("//Alice", None).expect("dev key should parse");
    let faucet = account_id_from_pair(&faucet_pair);

    let claimant_pair =
        sr25519::Pair::from_string("//Charlie", None).expect("dev key should parse");
    let claimant = account_id_from_pair(&claimant_pair);

    new_test_ext_with_faucet(&faucet, 1u128 << 60).execute_with(|| {
        System::set_block_number(1);
        let genesis_hash = System::block_hash(0);

        // Sanity: this account truly has no funds in our genesis.
        assert_eq!(Balances::free_balance(&claimant), 0);

        let call = RuntimeCall::Balances(pallet_balances::Call::transfer_keep_alive {
            dest: faucet.clone().into(),
            value: 1,
        });

        let xt = signed_extrinsic(call, &claimant_pair, 0, genesis_hash, genesis_hash);

        let validity =
            Executive::validate_transaction(TransactionSource::External, xt, genesis_hash);

        assert_eq!(
            validity,
            Err(TransactionValidityError::Invalid(
                InvalidTransaction::Payment
            )),
            "expected zero-balance account to be rejected for non-faucet call, got: {validity:?}"
        );
    });
}
