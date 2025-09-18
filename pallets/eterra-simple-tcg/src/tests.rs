use frame_support::{
	dispatch::DispatchResult,
	pallet_prelude::*,
	traits::Get,
};
use frame_system::pallet_prelude::*;
use sp_runtime::traits::{AtLeast32BitUnsigned, Bounded, CheckedAdd, CheckedSub, MaybeSerializeDeserialize, Member, One};
use sp_std::vec::Vec;

#[frame_support::pallet]
pub mod pallet {
	use super::*;

	#[pallet::config]
	pub trait Config: frame_system::Config {
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;
		type CardId: Member + Parameter + AtLeast32BitUnsigned + Default + Copy + MaybeSerializeDeserialize + Bounded;
		#[pallet::constant]
		type OwnedLimit: Get<u32>;
	}

	#[pallet::pallet]
	#[pallet::without_storage_info]
	pub struct Pallet<T>(_);

	#[pallet::storage]
	#[pallet::getter(fn cards)]
	pub type Cards<T: Config> = StorageMap<_, Blake2_128Concat, T::CardId, Card<T::AccountId>>;

	#[pallet::storage]
	#[pallet::getter(fn owned_cards)]
	pub type OwnedCards<T: Config> = StorageMap<_, Blake2_128Concat, T::AccountId, Vec<T::CardId>, ValueQuery>;

	#[pallet::storage]
	#[pallet::getter(fn next_card_id)]
	pub type NextCardId<T: Config> = StorageValue<_, T::CardId, ValueQuery>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		CardMinted { player: T::AccountId, card_id: T::CardId },
		CardTransferred { from: T::AccountId, to: T::AccountId, card_id: T::CardId },
	}

	#[pallet::error]
	pub enum Error<T> {
		NotCardOwner,
		NoSuchCard,
		OwnedLimitReached,
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		#[pallet::weight(10_000)]
		pub fn mint_card(origin: OriginFor<T>) -> DispatchResult {
			let who = ensure_signed(origin)?;
			Self::create_new_card(&who)?;
			Ok(())
		}

		#[pallet::weight(10_000)]
		pub fn transfer_card(origin: OriginFor<T>, card_id: T::CardId, to: T::AccountId) -> DispatchResult {
			let who = ensure_signed(origin)?;
			let mut card = Cards::<T>::get(card_id).ok_or(Error::<T>::NoSuchCard)?;

			ensure!(card.owner == who, Error::<T>::NotCardOwner);

			// Remove card from current owner's list
			OwnedCards::<T>::try_mutate(&who, |list| {
				if let Some(pos) = list.iter().position(|&id| id == card_id) {
					list.swap_remove(pos);
					Ok(())
				} else {
					Err(Error::<T>::NoSuchCard)
				}
			})?;

			// Add card to new owner's list
			OwnedCards::<T>::try_mutate(&to, |list| {
				if list.len() as u32 >= <T as Config>::OwnedLimit::get() {
					return Err(Error::<T>::OwnedLimitReached);
				}
				list.push(card_id);
				Ok(())
			})?;

			// Update card owner
			card.owner = to.clone();
			Cards::<T>::insert(card_id, card);

			Self::deposit_event(Event::CardTransferred { from: who, to, card_id });

			Ok(())
		}
	}

	impl<T: Config> Pallet<T> {
		fn create_new_card(owner: &T::AccountId) -> DispatchResult {
			let card_id = NextCardId::<T>::get();
			let next_id = card_id.checked_add(&One::one()).ok_or(Error::<T>::OwnedLimitReached)?;

			OwnedCards::<T>::try_mutate(owner, |list| {
				if list.len() as u32 >= <T as Config>::OwnedLimit::get() {
					return Err(Error::<T>::OwnedLimitReached);
				}
				list.push(card_id);
				Ok(())
			})?;

			let card = Card { owner: owner.clone() };
			Cards::<T>::insert(card_id, card);
			NextCardId::<T>::put(next_id);

			Self::deposit_event(Event::CardMinted { player: owner.clone(), card_id });

			Ok(())
		}
	}

	#[derive(Clone, Encode, Decode, PartialEq, RuntimeDebug, TypeInfo)]
	pub struct Card<AccountId> {
		pub owner: AccountId,
	}
}

#[cfg(test)]
mod tests {
    use super::pallet as test_pallet;
    use super::*;
    use frame_support::{assert_noop, assert_ok, parameter_types, traits::ConstU32};
    use frame_system as system;
    use sp_runtime::BuildStorage;

    type UncheckedExtrinsic = system::mocking::MockUncheckedExtrinsic<Test>;
    type Block = system::mocking::MockBlock<Test>;

    frame_support::construct_runtime!(
        pub enum Test where
            Block = Block,
            NodeBlock = Block,
            UncheckedExtrinsic = UncheckedExtrinsic,
        {
            System: frame_system,
            Cards: test_pallet,
        }
    );

    parameter_types! {
        pub const BlockHashCount: u64 = 250;
        pub const OwnedCap: u32 = 2; // small cap to test limit behavior
    }

    impl system::Config for Test {
        type BaseCallFilter = frame_support::traits::Everything;
        type BlockWeights = ();
        type BlockLength = ();
        type DbWeight = ();
        type RuntimeOrigin = RuntimeOrigin;
        type Nonce = u64;
        type Hash = sp_core::H256;
        type Hashing = sp_runtime::traits::BlakeTwo256;
        type AccountId = u64;
        type Lookup = sp_runtime::traits::IdentityLookup<Self::AccountId>;
        type RuntimeEvent = RuntimeEvent;
        type RuntimeCall = RuntimeCall;
        type RuntimeTask = (); // not used
        type PalletInfo = PalletInfo;
        type AccountData = ();
        type OnNewAccount = ();
        type OnKilledAccount = ();
        type SystemWeightInfo = ();
        type SS58Prefix = frame_support::traits::ConstU16<42>;
        type OnSetCode = ();
        type MaxConsumers = frame_support::traits::ConstU32<16>;
        type Block = Block;
        type BlockHashCount = BlockHashCount;
        type Version = ();
        type SingleBlockMigrations = ();
        type MultiBlockMigrator = ();
        type PreInherents = ();
        type PostInherents = ();
        type PostTransactions = ();
    }

    impl test_pallet::Config for Test {
        type RuntimeEvent = RuntimeEvent;
        type CardId = u32;
        type OwnedLimit = OwnedCap;
    }

    fn new_test_ext() -> sp_io::TestExternalities {
        let storage = system::GenesisConfig::<Test>::default()
            .build_storage()
            .unwrap();
        storage.into()
    }

    #[test]
    fn mint_card_emits_event_and_updates_owned() {
        new_test_ext().execute_with(|| {
            let player = 10;
            System::reset_events();
            System::set_block_number(1);

            assert_ok!(Cards::mint_card(RuntimeOrigin::signed(player)));

            // Owned list
            let ids = Cards::owned_cards(player);
            assert_eq!(ids.len(), 1);
            let card_id = ids[0];

            // Card storage
            let card = Cards::cards(card_id).expect("card exists");
            assert_eq!(card.owner, player);

            // Event
            System::assert_has_event(RuntimeEvent::Cards(test_pallet::Event::CardMinted { player, card_id }));
        });
    }

    #[test]
    fn transfer_card_success_moves_owner_and_index() {
        new_test_ext().execute_with(|| {
            System::set_block_number(1);
            let a = 1; let b = 2;
            assert_ok!(Cards::mint_card(RuntimeOrigin::signed(a)));
            let id = Cards::owned_cards(a)[0];

            assert_ok!(Cards::transfer_card(RuntimeOrigin::signed(a), id, b));

            // owner changed
            let card = Cards::cards(id).unwrap();
            assert_eq!(card.owner, b);

            // indexes updated
            assert!(!Cards::owned_cards(a).iter().any(|&x| x == id));
            assert!(Cards::owned_cards(b).iter().any(|&x| x == id));

            // event present
            let found = System::events().iter().any(|r| matches!(
                r.event,
                RuntimeEvent::Cards(test_pallet::Event::CardTransferred{ from, to, card_id })
                    if from == a && to == b && card_id == id
            ));
            assert!(found, "CardTransferred not found");
        });
    }

    #[test]
    fn transfer_card_by_non_owner_fails() {
        new_test_ext().execute_with(|| {
            let owner = 1; let non = 2; let to = 3;
            assert_ok!(Cards::mint_card(RuntimeOrigin::signed(owner)));
            let id = Cards::owned_cards(owner)[0];

            assert_noop!(Cards::transfer_card(RuntimeOrigin::signed(non), id, to), test_pallet::Error::<Test>::NotCardOwner);
        });
    }

    #[test]
    fn transfer_nonexistent_card_fails() {
        new_test_ext().execute_with(|| {
            assert_noop!(Cards::transfer_card(RuntimeOrigin::signed(1), 9999, 2), test_pallet::Error::<Test>::NoSuchCard);
        });
    }

    #[test]
    fn mint_respects_owned_limit() {
        new_test_ext().execute_with(|| {
            let p = 1;
            // OwnedCap is 2 for this test runtime
            assert_ok!(Cards::mint_card(RuntimeOrigin::signed(p)));
            assert_ok!(Cards::mint_card(RuntimeOrigin::signed(p)));
            // Third should fail
            assert_noop!(Cards::mint_card(RuntimeOrigin::signed(p)), test_pallet::Error::<Test>::OwnedLimitReached);
        });
    }

    #[test]
    fn ids_increase_monotonically() {
        new_test_ext().execute_with(|| {
            let p = 1;
            assert_ok!(Cards::mint_card(RuntimeOrigin::signed(p)));
            assert_ok!(Cards::mint_card(RuntimeOrigin::signed(p)));
            let ids = Cards::owned_cards(p);
            assert_eq!(ids.len(), 2);
            assert!(ids[1] > ids[0]);
        });
    }
}
