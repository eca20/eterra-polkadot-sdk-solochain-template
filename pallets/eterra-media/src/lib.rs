#![cfg_attr(not(feature = "std"), no_std)]

//! Eterra media pallet
//!
//! Immutable media registry with collections, roles, and delivery hints.
//! Intended to pair cleanly with `pallet-nfts` and game pallets that
//! reference `MediaId` for artwork, audio, skins, etc.

pub use pallet::*;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

use codec::{Decode, Encode};
use scale_info::TypeInfo;
use sp_std::prelude::*;

#[frame_support::pallet]
pub mod pallet {
    use super::*;
    use frame_support::{
        pallet_prelude::*,
        traits::BuildGenesisConfig,
        BoundedBTreeSet, BoundedVec,
    };
    use frame_system::pallet_prelude::*;

    pub type MediaId = u64;
    pub type MediaCollectionId = u32;

    /// High-level class/channel of the media, for policy & filtering.
    #[derive(
        Encode, Decode, Clone, Copy, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen,
    )]
    pub enum MediaClass {
        CoreAsset,
        Cosmetic,
        AudioSfx,
        VoiceLine,
        Skin,
        UserBanner,
        Experimental,
        Other,
    }

    /// How this media is expected to be delivered to clients.
    #[derive(
        Encode, Decode, Clone, Copy, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen,
    )]
    pub enum Delivery {
        /// Shipped with the game client; URI is mostly an identifier.
        ClientBundled,
        /// Retrieved via IPFS (e.g., ipfs://CID).
        RemoteIpfs,
        /// Retrieved via plain HTTP/HTTPS or CDN.
        RemoteHttp,
    }

    /// Roles within a media collection; roughly analogous to pallet-nfts collection roles.
    #[derive(
        Encode, Decode, Clone, Copy, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen, Ord, PartialOrd,
    )]
    pub enum CollectionRole {
        Admin,
        Uploader,
        Curator,
    }

    /// Collection-level metadata.
    #[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
    pub struct CollectionInfo<AccountId, BStr> {
        pub owner: AccountId,
        pub name: BStr,
        /// Optional short description stored separately if needed.
        pub frozen: bool,
    }

    /// Core media metadata; immutable except for flags like `is_deprecated`.
    #[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
    pub struct MediaMetadata<BStrUri, BStrCt, AccountId> {
        pub collection_id: MediaCollectionId,
        pub owner: AccountId,
        pub uri: BStrUri,
        pub content_type: BStrCt,
        pub class: MediaClass,
        pub delivery: Delivery,
        pub size_bytes: Option<u64>,
        pub version: u32,
        pub is_deprecated: bool,
    }

    #[pallet::config]
    pub trait Config: frame_system::Config {
        /// The aggregated event type of the runtime.
        type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

        /// Max length for URI strings (CID or URL).
        #[pallet::constant]
        type MaxUriLen: Get<u32>;

        /// Max length for content-type strings (e.g. "image/png").
        #[pallet::constant]
        type MaxContentTypeLen: Get<u32>;

        /// Max length for collection/media names.
        #[pallet::constant]
        type MaxNameLen: Get<u32>;

        /// Max length for collection descriptions.
        #[pallet::constant]
        type MaxDescriptionLen: Get<u32>;

        /// Max number of roles a single account can have in a collection.
        #[pallet::constant]
        type MaxRolesPerAccount: Get<u32>;

        /// ID of the default collection used if a caller does not specify one.
        #[pallet::constant]
        type DefaultCollectionId: Get<MediaCollectionId>;

        /// Default owner account for the default collection when not overridden in genesis.
        type DefaultCollectionOwner: Get<Self::AccountId>;
    }

    // Convenient aliases
    type BoundedStrUri<T> =
        BoundedVec<u8, <T as Config>::MaxUriLen>;
    type BoundedStrContentType<T> =
        BoundedVec<u8, <T as Config>::MaxContentTypeLen>;
    type BoundedStrName<T> =
        BoundedVec<u8, <T as Config>::MaxNameLen>;
    type BoundedStrDescription<T> =
        BoundedVec<u8, <T as Config>::MaxDescriptionLen>;

    #[pallet::storage]
    #[pallet::getter(fn next_media_id)]
    pub type NextMediaId<T: Config> = StorageValue<_, MediaId, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn next_collection_id)]
    pub type NextCollectionId<T: Config> = StorageValue<_, MediaCollectionId, ValueQuery>;

    /// Collections registry.
    #[pallet::storage]
    #[pallet::getter(fn collection)]
    pub type Collections<T: Config> = StorageMap<
        _,
        Blake2_128Concat,
        MediaCollectionId,
        CollectionInfo<
            T::AccountId,
            BoundedStrName<T>,
        >,
        OptionQuery,
    >;

    /// Optional mapping from collection to full description.
    #[pallet::storage]
    #[pallet::getter(fn collection_description)]
    pub type CollectionDescriptions<T: Config> = StorageMap<
        _,
        Blake2_128Concat,
        MediaCollectionId,
        BoundedStrDescription<T>,
        OptionQuery,
    >;

    /// Collection roles per account.
    #[pallet::storage]
    #[pallet::getter(fn collection_roles)]
    pub type CollectionRoles<T: Config> = StorageDoubleMap<
        _,
        Blake2_128Concat, MediaCollectionId,
        Blake2_128Concat, T::AccountId,
        BoundedBTreeSet<CollectionRole, T::MaxRolesPerAccount>,
        ValueQuery,
    >;

    /// Core media registry.
    #[pallet::storage]
    #[pallet::getter(fn media)]
    pub type Media<T: Config> = StorageMap<
        _,
        Blake2_128Concat,
        MediaId,
        MediaMetadata<
            BoundedStrUri<T>,
            BoundedStrContentType<T>,
            T::AccountId,
        >,
        OptionQuery,
    >;

    /// Owner shortcut (redundant but handy).
    #[pallet::storage]
    #[pallet::getter(fn media_owner)]
    pub type MediaOwner<T: Config> = StorageMap<
        _,
        Blake2_128Concat,
        MediaId,
        T::AccountId,
        OptionQuery,
    >;

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        CollectionCreated {
            collection_id: MediaCollectionId,
            owner: T::AccountId,
        },
        CollectionRoleSet {
            collection_id: MediaCollectionId,
            account: T::AccountId,
            role: CollectionRole,
            granted: bool,
        },
        MediaRegistered {
            media_id: MediaId,
            owner: T::AccountId,
            collection_id: MediaCollectionId,
        },
        MediaDeprecated {
            media_id: MediaId,
        },
    }

    #[pallet::error]
    pub enum Error<T> {
        /// The specified collection does not exist.
        UnknownCollection,
        /// Caller is not allowed to perform this action in the collection.
        NoPermission,
        /// The collection is frozen and cannot accept new media.
        CollectionFrozen,
        /// URI string too long.
        UriTooLong,
        /// content-type string too long.
        ContentTypeTooLong,
        /// Name string too long.
        NameTooLong,
        /// Description string too long.
        DescriptionTooLong,
        /// Media item does not exist.
        UnknownMedia,
        /// Media is already deprecated.
        AlreadyDeprecated,
    }

    #[pallet::genesis_config]
    pub struct GenesisConfig<T: Config> {
        /// Whether to create the default collection at genesis.
        pub create_default_collection: bool,
        /// Name for the default collection.
        pub default_collection_name: Vec<u8>,
        /// Description for the default collection.
        pub default_collection_description: Vec<u8>,
        /// Owner for the default collection (if None, uses `T::DefaultCollectionOwner::get()`).
        pub default_collection_owner: Option<T::AccountId>,
    }

    impl<T: Config> Default for GenesisConfig<T> {
        fn default() -> Self {
            Self {
                create_default_collection: false,
                default_collection_name: b"Default Media".to_vec(),
                default_collection_description: b"Default media collection".to_vec(),
                default_collection_owner: None,
            }
        }
    }

    #[pallet::genesis_build]
    impl<T: Config> BuildGenesisConfig for GenesisConfig<T> {
        fn build(&self) {
            NextMediaId::<T>::put(0u64);
            NextCollectionId::<T>::put(0u32);

            if self.create_default_collection {
                let id = T::DefaultCollectionId::get();
                let owner = self
                    .default_collection_owner
                    .clone()
                    .unwrap_or_else(T::DefaultCollectionOwner::get);

                let name: BoundedStrName<T> = self
                    .default_collection_name
                    .clone()
                    .try_into()
                    .unwrap_or_default();

                let desc: BoundedStrDescription<T> = self
                    .default_collection_description
                    .clone()
                    .try_into()
                    .unwrap_or_default();

                let info = CollectionInfo {
                    owner: owner.clone(),
                    name,
                    frozen: false,
                };

                Collections::<T>::insert(id, info);
                CollectionDescriptions::<T>::insert(id, desc);

                // Grant Admin + Uploader by default.
                let mut roles = BoundedBTreeSet::new();
                let _ = roles.try_insert(CollectionRole::Admin);
                let _ = roles.try_insert(CollectionRole::Uploader);
                CollectionRoles::<T>::insert(id, owner, roles);
            }
        }
    }

    #[pallet::pallet]
    pub struct Pallet<T>(_);

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        /// Create a new media collection.
        #[pallet::weight(10_000)]
        pub fn create_collection(
            origin: OriginFor<T>,
            name: Vec<u8>,
            description: Vec<u8>,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;

            let bounded_name: BoundedStrName<T> = name
                .try_into()
                .map_err(|_| Error::<T>::NameTooLong)?;
            let bounded_desc: BoundedStrDescription<T> = description
                .try_into()
                .map_err(|_| Error::<T>::DescriptionTooLong)?;

            let id = NextCollectionId::<T>::get();
            NextCollectionId::<T>::put(id.checked_add(1).expect("overflow in collection id"));

            let info = CollectionInfo {
                owner: who.clone(),
                name: bounded_name,
                frozen: false,
            };

            Collections::<T>::insert(id, info);
            CollectionDescriptions::<T>::insert(id, bounded_desc.clone());

            // By default caller is Admin + Uploader.
            let mut roles = BoundedBTreeSet::new();
            let _ = roles.try_insert(CollectionRole::Admin);
            let _ = roles.try_insert(CollectionRole::Uploader);
            CollectionRoles::<T>::insert(id, &who, roles);

            Self::deposit_event(Event::CollectionCreated { collection_id: id, owner: who });
            Ok(())
        }

        /// Set or unset a role for an account in a collection.
        /// Only a collection Admin may call this.
        #[pallet::weight(10_000)]
        pub fn set_collection_role(
            origin: OriginFor<T>,
            collection_id: MediaCollectionId,
            account: T::AccountId,
            role: CollectionRole,
            granted: bool,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;

            let caller_roles = CollectionRoles::<T>::get(collection_id, &who);
            ensure!(
                caller_roles.contains(&CollectionRole::Admin),
                Error::<T>::NoPermission
            );

            let mut roles = CollectionRoles::<T>::get(collection_id, &account);
            if granted {
                let _ = roles.try_insert(role);
            } else {
                roles.remove(&role);
            }
            CollectionRoles::<T>::insert(collection_id, &account, roles);

            Self::deposit_event(Event::CollectionRoleSet {
                collection_id,
                account,
                role,
                granted,
            });
            Ok(())
        }

        /// Register a new immutable media item.
        ///
        /// If `maybe_collection_id` is `None`, the pallet will fall back to `Config::DefaultCollectionId`.
        #[pallet::weight(10_000)]
        pub fn register_media(
            origin: OriginFor<T>,
            maybe_collection_id: Option<MediaCollectionId>,
            uri: Vec<u8>,
            content_type: Vec<u8>,
            class: MediaClass,
            delivery: Delivery,
            size_bytes: Option<u64>,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;

            let collection_id = maybe_collection_id
                .unwrap_or_else(|| T::DefaultCollectionId::get());

            // Ensure collection exists.
            let collection_info = Collections::<T>::get(collection_id)
                .ok_or(Error::<T>::UnknownCollection)?;

            ensure!(!collection_info.frozen, Error::<T>::CollectionFrozen);

            // Permission: owner or has Uploader role.
            let roles = CollectionRoles::<T>::get(collection_id, &who);
            let is_owner = collection_info.owner == who;
            let can_upload = roles.contains(&CollectionRole::Uploader);

            ensure!(is_owner || can_upload, Error::<T>::NoPermission);

            let bounded_uri: BoundedStrUri<T> = uri
                .try_into()
                .map_err(|_| Error::<T>::UriTooLong)?;
            let bounded_ct: BoundedStrContentType<T> = content_type
                .try_into()
                .map_err(|_| Error::<T>::ContentTypeTooLong)?;

            let media_id = NextMediaId::<T>::get();
            NextMediaId::<T>::put(media_id.checked_add(1).expect("overflow in media id"));

            let metadata = MediaMetadata {
                collection_id,
                owner: who.clone(),
                uri: bounded_uri,
                content_type: bounded_ct,
                class,
                delivery,
                size_bytes,
                version: 1,
                is_deprecated: false,
            };

            Media::<T>::insert(media_id, metadata);
            MediaOwner::<T>::insert(media_id, &who);

            Self::deposit_event(Event::MediaRegistered {
                media_id,
                owner: who,
                collection_id,
            });

            Ok(())
        }

                /// Freeze a collection, preventing new media from being registered.
        /// Only collection Admin or owner may do this.
        #[pallet::weight(10_000)]
        pub fn freeze_collection(
            origin: OriginFor<T>,
            collection_id: MediaCollectionId,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;

            Collections::<T>::try_mutate(collection_id, |maybe_info| -> DispatchResult {
                let info = maybe_info.as_mut().ok_or(Error::<T>::UnknownCollection)?;

                let roles = CollectionRoles::<T>::get(collection_id, &who);
                let is_owner = info.owner == who;
                let is_admin = roles.contains(&CollectionRole::Admin);

                ensure!(is_owner || is_admin, Error::<T>::NoPermission);

                info.frozen = true;
                Ok(())
            })
        }

        /// Mark a media item as deprecated.
        /// Only collection Admin or media owner may do this.
        #[pallet::weight(10_000)]
        pub fn deprecate_media(
            origin: OriginFor<T>,
            media_id: MediaId,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;

            Media::<T>::try_mutate(media_id, |maybe_meta| -> DispatchResult {
                let meta = maybe_meta.as_mut().ok_or(Error::<T>::UnknownMedia)?;
                ensure!(!meta.is_deprecated, Error::<T>::AlreadyDeprecated);

                let collection_id = meta.collection_id;
                let roles = CollectionRoles::<T>::get(collection_id, &who);
                let is_owner = meta.owner == who;
                let is_admin = roles.contains(&CollectionRole::Admin);

                ensure!(is_owner || is_admin, Error::<T>::NoPermission);

                meta.is_deprecated = true;
                Ok(())
            })?;

            Self::deposit_event(Event::MediaDeprecated { media_id });
            Ok(())
        }
    }
}
