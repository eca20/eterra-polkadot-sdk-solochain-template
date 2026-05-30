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
use sp_std::vec::Vec;

/// Minimal interface for other pallets to grant experience to an account.
///
/// This avoids needing to dispatch the privileged `grant_experience` extrinsic from within
/// runtime logic.
pub trait ExperienceManager<AccountId> {
    fn grant_experience(to: &AccountId, amount: u128);
}

#[frame_support::pallet]
pub mod pallet {
    use super::*;
    use crate::weights::WeightInfo;
    use frame_support::traits::StorageVersion;

    type BalanceOf<T> =
        <<T as Config>::Currency as Currency<<T as frame_system::Config>::AccountId>>::Balance;

    #[pallet::config]
    pub trait Config: frame_system::Config {
        /// Runtime currency (native token).
        type Currency: Currency<Self::AccountId>;

        /// Canonical Alpha access gate for player-facing calls.
        type AccessControl: AccessControl<Self::AccountId>;

        /// Origin allowed to mint/grant XP (e.g., Root or a custom EnsureOrigin).
        type ExpIssuerOrigin: EnsureOrigin<Self::RuntimeOrigin>;

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
    }

    const STORAGE_VERSION: StorageVersion = StorageVersion::new(1);

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
            T::AccessControl::ensure_whitelisted(&who)?;
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
            T::AccessControl::ensure_whitelisted(&who)?;
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
            T::AccessControl::ensure_whitelisted(&who)?;
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
