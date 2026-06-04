#![cfg_attr(not(feature = "std"), no_std)]
#![allow(clippy::duplicated_attributes)]
pub use pallet::*;

pub mod weights;
pub use weights::WeightInfo;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

use frame_support::{
    pallet_prelude::*,
    traits::{Currency, ExistenceRequirement},
};
use frame_system::pallet_prelude::*;
use pallet_alpha_access::AccessControl;
use sp_core::sr25519;
use sp_std::vec::Vec;

pub type SteamHash = [u8; 32];
pub type SteamLinkNonce = [u8; 32];
pub type ReasonHash = [u8; 32];

#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub struct GamerProfile<BlockNumber> {
    pub linked_at: BlockNumber,
    pub frozen: bool,
    pub freeze_reason: Option<ReasonHash>,
}

/// Minimal interface for other pallets to grant experience to an account.
///
/// This avoids needing to dispatch the privileged `grant_experience` extrinsic from within
/// runtime logic.
pub trait ExperienceManager<AccountId> {
    fn grant_experience(to: &AccountId, amount: u128);
}

pub trait SteamIdentityProvider<AccountId> {
    fn account_for_steam_hash(steam_hash: SteamHash) -> Option<AccountId>;
    fn steam_hash_for_account(account: &AccountId) -> Option<SteamHash>;
    fn is_frozen(account: &AccountId) -> bool;
}

#[frame_support::pallet]
pub mod pallet {
    use super::*;
    use crate::weights::WeightInfo;
    use frame_support::traits::StorageVersion;

    type BalanceOf<T> =
        <<T as Config>::Currency as Currency<<T as frame_system::Config>::AccountId>>::Balance;
    type GamerProfileOf<T> = GamerProfile<BlockNumberFor<T>>;

    #[pallet::config]
    pub trait Config: frame_system::Config {
        /// Runtime currency (native token).
        type Currency: Currency<Self::AccountId>;

        /// Canonical Alpha access gate for player-facing calls.
        type AccessControl: AccessControl<Self::AccountId>;

        /// Origin allowed to mint/grant XP (e.g., Root or a custom EnsureOrigin).
        type ExpIssuerOrigin: EnsureOrigin<Self::RuntimeOrigin>;

        /// Origin allowed to administer Steam link authority and player freezes.
        type AdminOrigin: EnsureOrigin<Self::RuntimeOrigin>;

        /// Account that receives change fees (e.g., faucet/treasury account).
        #[pallet::constant]
        type FaucetAccount: Get<Self::AccountId>;

        /// The fee to change gamer tag or avatar after the first set.
        #[pallet::constant]
        type ChangeFee: Get<BalanceOf<Self>>;

        /// Maximum bytes for a gamer tag (e.g., 32).
        #[pallet::constant]
        type MaxTagLen: Get<u32>;

        /// Maximum bytes for avatar CID (e.g., 96 or 128). CIDs are ASCII bytes.
        #[pallet::constant]
        type MaxAvatarCidLen: Get<u32>;

        /// Maximum bytes for Steam link authority signatures.
        #[pallet::constant]
        type MaxSteamLinkSignatureLen: Get<u32>;

        /// Runtime event
        type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

        /// Weight information for extrinsics.
        type WeightInfo: WeightInfo;
    }

    #[pallet::storage]
    #[pallet::getter(fn tag)]
    /// Stored as raw UTF-8 bytes (bounded). First set is free; later changes cost a fee.
    pub type GamerTag<T: Config> =
        StorageMap<_, Blake2_128Concat, T::AccountId, BoundedVec<u8, T::MaxTagLen>, OptionQuery>;

    #[pallet::storage]
    #[pallet::getter(fn avatar_cid)]
    /// Stored as **ASCII** bytes representing a CID (IPFS / multibase). First set free; changes cost a fee.
    pub type AvatarCid<T: Config> = StorageMap<
        _,
        Blake2_128Concat,
        T::AccountId,
        BoundedVec<u8, T::MaxAvatarCidLen>,
        OptionQuery,
    >;

    /// Unredeemed experience points available to redeem.
    #[pallet::storage]
    #[pallet::getter(fn exp)]
    pub type Experience<T: Config> =
        StorageMap<_, Blake2_128Concat, T::AccountId, u128, ValueQuery>;

    /// Current level (0..=99).
    #[pallet::storage]
    #[pallet::getter(fn level)]
    pub type Level<T: Config> = StorageMap<_, Blake2_128Concat, T::AccountId, u8, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn steam_to_account)]
    pub type SteamToAccount<T: Config> =
        StorageMap<_, Blake2_128Concat, SteamHash, T::AccountId, OptionQuery>;

    #[pallet::storage]
    #[pallet::getter(fn account_to_steam)]
    pub type AccountToSteam<T: Config> =
        StorageMap<_, Blake2_128Concat, T::AccountId, SteamHash, OptionQuery>;

    #[pallet::storage]
    #[pallet::getter(fn gamer_profile)]
    pub type GamerProfiles<T: Config> =
        StorageMap<_, Blake2_128Concat, T::AccountId, GamerProfileOf<T>, OptionQuery>;

    #[pallet::storage]
    pub type UsedSteamLinkNonces<T: Config> =
        StorageMap<_, Blake2_128Concat, SteamLinkNonce, (), OptionQuery>;

    #[pallet::storage]
    #[pallet::getter(fn steam_link_authority)]
    pub type SteamLinkAuthority<T: Config> = StorageValue<_, [u8; 32], OptionQuery>;

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        TagSet {
            who: T::AccountId,
            tag: Vec<u8>,
            charged: bool,
        },
        AvatarSet {
            who: T::AccountId,
            cid: Vec<u8>,
            charged: bool,
        },
        ExperienceGranted {
            to: T::AccountId,
            amount: u128,
        },
        LevelUp {
            who: T::AccountId,
            new_level: u8,
        },
        SteamLinkAuthoritySet {
            authority_pubkey: [u8; 32],
        },
        SteamLinked {
            steam_hash: SteamHash,
            account: T::AccountId,
        },
        SteamUnlinked {
            steam_hash: SteamHash,
            account: T::AccountId,
        },
        PlayerFrozen {
            account: T::AccountId,
            reason_hash: ReasonHash,
        },
        PlayerUnfrozen {
            account: T::AccountId,
        },
    }

    #[pallet::error]
    pub enum Error<T> {
        TagTooShort,
        TagTooLong,
        AvatarCidTooLong,
        AvatarCidInvalidAscii,
        AlreadyMaxLevel,
        NotEnoughExperience,
        InsufficientBalanceForChange,
        InvalidLevelRequest,
        SteamLinkAuthorityNotSet,
        InvalidSteamLinkAuthority,
        InvalidSteamLinkSignature,
        SteamLinkExpired,
        SteamLinkNonceUsed,
        AlreadyLinked,
        SteamHashAlreadyLinked,
        SteamHashNotLinked,
        PlayerProfileNotFound,
        PlayerFrozen,
    }

    const STORAGE_VERSION: StorageVersion = StorageVersion::new(2);

    #[pallet::pallet]
    #[pallet::storage_version(STORAGE_VERSION)]
    pub struct Pallet<T>(_);

    #[pallet::hooks]
    impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
        fn on_runtime_upgrade() -> Weight {
            let mut weight = T::DbWeight::get().reads(1);
            if StorageVersion::get::<Pallet<T>>() < STORAGE_VERSION {
                STORAGE_VERSION.put::<Pallet<T>>();
                weight = weight.saturating_add(T::DbWeight::get().writes(1));
            }
            weight
        }
    }

    impl<T: Config> Pallet<T> {
        /// Small ASCII validation for CIDs: non-empty, only visible ASCII (33..=126).
        #[inline]
        fn validate_ascii_cid(cid: &[u8]) -> bool {
            if cid.is_empty() {
                return false;
            }
            // Avoid spaces/control characters; multibase CIDs are visible ASCII.
            cid.iter().all(|b| (33..=126).contains(b))
        }

        /// Required EXP to go from (level L-1) → L. L ∈ [1..99].
        /// Model:
        /// - L=1 requires exactly 250 EXP
        /// - L≥2 uses: 250 + round(k * (L² - 1))
        /// - k chosen so total EXP from 1..99 ≈ 1_000_000_000
        #[inline]
        pub fn exp_required_for_level(l: u8) -> u128 {
            let l = l as u128;
            if l == 1 {
                return 250;
            }
            // k ≈ 3046.3738115 ≈ NUM / DEN to avoid floats in no_std.
            const K_NUM: u128 = 3_046_373_812;
            const K_DEN: u128 = 1_000_000;

            let term = l * l - 1; // (L^2 - 1)
            let k_term = (K_NUM.saturating_mul(term).saturating_add(K_DEN / 2)) / K_DEN;
            250u128 + k_term
        }

        /// Try to redeem as many levels as EXP allows (capped at 99).
        fn redeem_all_levels(mut lvl: u8, mut xp: u128) -> (u8, u128, u8) {
            let mut gained = 0u8;
            while lvl < 99 {
                let need = Self::exp_required_for_level(lvl + 1);
                if xp < need {
                    break;
                }
                xp -= need;
                lvl = lvl.saturating_add(1);
                gained = gained.saturating_add(1);
            }
            (lvl, xp, gained)
        }

        fn charge_change_fee_if_needed(
            who: &T::AccountId,
            already_set: bool,
        ) -> Result<bool, Error<T>> {
            if !already_set {
                return Ok(false);
            }
            let fee = T::ChangeFee::get();
            T::Currency::transfer(
                who,
                &T::FaucetAccount::get(),
                fee,
                ExistenceRequirement::KeepAlive,
            )
            .map_err(|_| Error::<T>::InsufficientBalanceForChange)?;
            Ok(true)
        }

        fn ensure_profile_access(who: &T::AccountId) -> DispatchResult {
            if AccountToSteam::<T>::contains_key(who) {
                Self::ensure_account_not_frozen(who)?;
                return Ok(());
            }
            T::AccessControl::ensure_whitelisted(who)
        }

        fn ensure_account_not_frozen(who: &T::AccountId) -> DispatchResult {
            if let Some(profile) = GamerProfiles::<T>::get(who) {
                ensure!(!profile.frozen, Error::<T>::PlayerFrozen);
            }
            Ok(())
        }

        fn steam_link_payload(
            account: &T::AccountId,
            steam_hash: &SteamHash,
            nonce: &SteamLinkNonce,
            expires_at: &BlockNumberFor<T>,
        ) -> Vec<u8> {
            let mut payload = b"eterra:gamer:steam-link:v1".to_vec();
            account.encode_to(&mut payload);
            steam_hash.encode_to(&mut payload);
            nonce.encode_to(&mut payload);
            expires_at.encode_to(&mut payload);
            payload
        }

        fn verify_steam_link_signature(
            account: &T::AccountId,
            steam_hash: &SteamHash,
            nonce: &SteamLinkNonce,
            expires_at: &BlockNumberFor<T>,
            signature: &[u8],
        ) -> Result<(), Error<T>> {
            let authority =
                SteamLinkAuthority::<T>::get().ok_or(Error::<T>::SteamLinkAuthorityNotSet)?;
            let signature = sr25519::Signature::try_from(signature)
                .map_err(|_| Error::<T>::InvalidSteamLinkSignature)?;
            let public = sr25519::Public::from_raw(authority);
            let payload = Self::steam_link_payload(account, steam_hash, nonce, expires_at);
            ensure!(
                sp_io::crypto::sr25519_verify(&signature, &payload, &public),
                Error::<T>::InvalidSteamLinkSignature
            );
            Ok(())
        }
    }

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        /// Set (or change) gamer tag. First set is free; changes cost 100 tokens (configurable).
        #[pallet::call_index(0)]
        #[pallet::weight(T::WeightInfo::set_gamer_tag())]
        pub fn set_gamer_tag(
            origin: OriginFor<T>,
            tag: BoundedVec<u8, T::MaxTagLen>,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            Self::ensure_profile_access(&who)?;
            ensure!(!tag.is_empty(), Error::<T>::TagTooShort);
            let tag_raw = tag.to_vec();

            let already = <GamerTag<T>>::contains_key(&who);
            let charged = Self::charge_change_fee_if_needed(&who, already)?;

            <GamerTag<T>>::insert(&who, tag);
            Self::deposit_event(Event::TagSet {
                who,
                tag: tag_raw,
                charged,
            });
            Ok(())
        }

        /// Set (or change) avatar CID (e.g., IPFS). First set free; changes cost 100 tokens (configurable).
        /// The value must be printable ASCII (no spaces/control chars) and within MaxAvatarCidLen.
        #[pallet::call_index(1)]
        #[pallet::weight(T::WeightInfo::set_avatar())]
        pub fn set_avatar(
            origin: OriginFor<T>,
            cid: BoundedVec<u8, T::MaxAvatarCidLen>,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            Self::ensure_profile_access(&who)?;
            ensure!(
                Self::validate_ascii_cid(&cid),
                Error::<T>::AvatarCidInvalidAscii
            );
            let cid_raw = cid.to_vec();

            let already = <AvatarCid<T>>::contains_key(&who);
            let charged = Self::charge_change_fee_if_needed(&who, already)?;

            <AvatarCid<T>>::insert(&who, cid);
            Self::deposit_event(Event::AvatarSet {
                who,
                cid: cid_raw,
                charged,
            });
            Ok(())
        }

        /// (Privileged) Grant experience to a player (minting XP).
        #[pallet::call_index(2)]
        #[pallet::weight(T::WeightInfo::grant_experience())]
        pub fn grant_experience(
            origin: OriginFor<T>,
            to: T::AccountId,
            amount: u128,
        ) -> DispatchResult {
            T::ExpIssuerOrigin::ensure_origin(origin)?;
            Experience::<T>::mutate(&to, |xp| *xp = xp.saturating_add(amount));
            Self::deposit_event(Event::ExperienceGranted { to, amount });
            Ok(())
        }

        /// Redeem available experience into levels until you run out of EXP or hit 99.
        #[pallet::call_index(3)]
        #[pallet::weight(T::WeightInfo::redeem_levels())]
        pub fn redeem_levels(origin: OriginFor<T>) -> DispatchResult {
            let who = ensure_signed(origin)?;
            Self::ensure_profile_access(&who)?;
            let current = Level::<T>::get(&who);
            ensure!(current <= 99, Error::<T>::InvalidLevelRequest);
            ensure!(current < 99, Error::<T>::AlreadyMaxLevel);

            let xp = Experience::<T>::get(&who);
            let (new_level, new_xp, gained) = Self::redeem_all_levels(current, xp);
            ensure!(gained > 0, Error::<T>::NotEnoughExperience);

            Level::<T>::insert(&who, new_level);
            Experience::<T>::insert(&who, new_xp);
            Self::deposit_event(Event::LevelUp { who, new_level });
            Ok(())
        }

        #[pallet::call_index(4)]
        #[pallet::weight(T::WeightInfo::set_steam_link_authority())]
        pub fn set_steam_link_authority(
            origin: OriginFor<T>,
            authority_pubkey: [u8; 32],
        ) -> DispatchResult {
            T::AdminOrigin::ensure_origin(origin)?;
            ensure!(
                authority_pubkey.iter().any(|byte| *byte != 0),
                Error::<T>::InvalidSteamLinkAuthority
            );
            SteamLinkAuthority::<T>::put(authority_pubkey);
            Self::deposit_event(Event::SteamLinkAuthoritySet { authority_pubkey });
            Ok(())
        }

        #[pallet::call_index(5)]
        #[pallet::weight(T::WeightInfo::link_steam())]
        pub fn link_steam(
            origin: OriginFor<T>,
            steam_hash: SteamHash,
            nonce: SteamLinkNonce,
            expires_at: BlockNumberFor<T>,
            authority_signature: BoundedVec<u8, T::MaxSteamLinkSignatureLen>,
        ) -> DispatchResult {
            let account = ensure_signed(origin)?;
            let now = frame_system::Pallet::<T>::block_number();
            ensure!(expires_at > now, Error::<T>::SteamLinkExpired);
            ensure!(
                !UsedSteamLinkNonces::<T>::contains_key(nonce),
                Error::<T>::SteamLinkNonceUsed
            );
            ensure!(
                !AccountToSteam::<T>::contains_key(&account),
                Error::<T>::AlreadyLinked
            );
            ensure!(
                !SteamToAccount::<T>::contains_key(steam_hash),
                Error::<T>::SteamHashAlreadyLinked
            );
            Self::verify_steam_link_signature(
                &account,
                &steam_hash,
                &nonce,
                &expires_at,
                authority_signature.as_slice(),
            )?;

            SteamToAccount::<T>::insert(steam_hash, &account);
            AccountToSteam::<T>::insert(&account, steam_hash);
            GamerProfiles::<T>::insert(
                &account,
                GamerProfile {
                    linked_at: now,
                    frozen: false,
                    freeze_reason: None,
                },
            );
            UsedSteamLinkNonces::<T>::insert(nonce, ());
            Self::deposit_event(Event::SteamLinked {
                steam_hash,
                account,
            });
            Ok(())
        }

        #[pallet::call_index(6)]
        #[pallet::weight(T::WeightInfo::unlink_steam())]
        pub fn unlink_steam(origin: OriginFor<T>) -> DispatchResult {
            let account = ensure_signed(origin)?;
            let steam_hash =
                AccountToSteam::<T>::take(&account).ok_or(Error::<T>::SteamHashNotLinked)?;
            SteamToAccount::<T>::remove(steam_hash);
            GamerProfiles::<T>::remove(&account);
            Self::deposit_event(Event::SteamUnlinked {
                steam_hash,
                account,
            });
            Ok(())
        }

        #[pallet::call_index(7)]
        #[pallet::weight(T::WeightInfo::freeze_player())]
        pub fn freeze_player(
            origin: OriginFor<T>,
            account: T::AccountId,
            reason_hash: ReasonHash,
        ) -> DispatchResult {
            T::AdminOrigin::ensure_origin(origin)?;
            GamerProfiles::<T>::try_mutate(&account, |maybe_profile| -> DispatchResult {
                let profile = maybe_profile
                    .as_mut()
                    .ok_or(Error::<T>::PlayerProfileNotFound)?;
                profile.frozen = true;
                profile.freeze_reason = Some(reason_hash);
                Ok(())
            })?;
            Self::deposit_event(Event::PlayerFrozen {
                account,
                reason_hash,
            });
            Ok(())
        }

        #[pallet::call_index(8)]
        #[pallet::weight(T::WeightInfo::unfreeze_player())]
        pub fn unfreeze_player(origin: OriginFor<T>, account: T::AccountId) -> DispatchResult {
            T::AdminOrigin::ensure_origin(origin)?;
            GamerProfiles::<T>::try_mutate(&account, |maybe_profile| -> DispatchResult {
                let profile = maybe_profile
                    .as_mut()
                    .ok_or(Error::<T>::PlayerProfileNotFound)?;
                profile.frozen = false;
                profile.freeze_reason = None;
                Ok(())
            })?;
            Self::deposit_event(Event::PlayerUnfrozen { account });
            Ok(())
        }
    }
}

impl<T: pallet::Config> ExperienceManager<T::AccountId> for pallet::Pallet<T> {
    fn grant_experience(to: &T::AccountId, amount: u128) {
        pallet::Experience::<T>::mutate(to, |xp| *xp = xp.saturating_add(amount));
        pallet::Pallet::<T>::deposit_event(pallet::Event::<T>::ExperienceGranted {
            to: to.clone(),
            amount,
        });
    }
}

impl<T: pallet::Config> SteamIdentityProvider<T::AccountId> for pallet::Pallet<T> {
    fn account_for_steam_hash(steam_hash: SteamHash) -> Option<T::AccountId> {
        pallet::SteamToAccount::<T>::get(steam_hash)
    }

    fn steam_hash_for_account(account: &T::AccountId) -> Option<SteamHash> {
        pallet::AccountToSteam::<T>::get(account)
    }

    fn is_frozen(account: &T::AccountId) -> bool {
        pallet::GamerProfiles::<T>::get(account)
            .map(|profile| profile.frozen)
            .unwrap_or(false)
    }
}
