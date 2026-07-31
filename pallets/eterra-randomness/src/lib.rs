#![cfg_attr(not(feature = "std"), no_std)]
// FRAME's generated hook glue currently triggers this lint in macro expansion.
#![allow(clippy::manual_inspect)]

pub use pallet::*;

pub mod weights;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

use codec::{Decode, Encode, MaxEncodedLen};
use eterra_drand_quicknet::round_at_unix_seconds;
use eterra_nexus_primitives::{EconomicRealm, Hash32, DRAND_QUICKNET_CHAIN_HASH};
use frame_support::{dispatch::DispatchResult, pallet_prelude::*, traits::UnixTime};
use scale_info::TypeInfo;
use sp_runtime::{DispatchError, RuntimeDebug};

pub type RandomnessRequestId = Hash32;

#[derive(
    Encode, Decode, Clone, Copy, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen, Default,
)]
pub enum RandomnessMode {
    #[default]
    #[codec(index = 0)]
    Disabled,
    #[codec(index = 1)]
    DeterministicPrivateAlpha,
    #[codec(index = 2)]
    DrandQuicknet,
}

#[derive(Encode, Decode, Clone, Copy, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub enum RequestStatus {
    #[codec(index = 0)]
    Pending,
    #[codec(index = 1)]
    Finalized,
    #[codec(index = 2)]
    TimedOut,
}

#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub struct RandomnessRequest<BlockNumber> {
    pub request_id: RandomnessRequestId,
    pub domain: Hash32,
    pub commitment: Hash32,
    pub immutable_config_hash: Hash32,
    pub exact_epoch: u64,
    pub requested_at: BlockNumber,
    pub not_before: BlockNumber,
    pub timeout_at: BlockNumber,
    pub mode: RandomnessMode,
    pub status: RequestStatus,
}

#[derive(Encode, Decode, Clone, Copy, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub struct VerifiedRandomnessOutput<BlockNumber> {
    pub epoch: u64,
    pub output: Hash32,
    pub proof_hash: Hash32,
    pub finalized_at: BlockNumber,
    pub deterministic_alpha: bool,
}

/// Immutable request context kept separately from the original request codec.
/// New consumers use this record to prevent a Training/alpha result from being
/// replayed into a Production decision.
#[derive(Encode, Decode, Clone, Copy, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub struct RandomnessRequestContext {
    pub economic_realm: EconomicRealm,
    pub expected_provenance: RandomnessMode,
    pub eterra_genesis_hash: Hash32,
    pub pallet_instance_id: u8,
    /// Quicknet chain hash for Drand requests; all zeroes for the explicitly
    /// non-economic deterministic-alpha fixture.
    pub provider_chain_hash: Hash32,
}

/// Provenance-preserving output view for economic consumers. The legacy tuple
/// remains available for callers that have not yet migrated, but Production
/// consumers must use this bound view.
#[derive(Encode, Decode, Clone, Copy, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub struct RealmBoundRandomnessOutput {
    pub epoch: u64,
    pub output: Hash32,
    pub proof_hash: Hash32,
    pub economic_realm: EconomicRealm,
    pub provenance: RandomnessMode,
    pub provider_chain_hash: Hash32,
}

pub trait DrandProofVerifier {
    fn verify_quicknet(chain_hash: &Hash32, round: u64, raw_signature: &[u8]) -> Option<Hash32>;
}

/// Runtime-supplied Eterra-chain context used in every new request ID. Keeping
/// this generic makes pallet tests cover cross-genesis and cross-instance
/// replay resistance without hard-coding a particular runtime layout.
pub trait RandomnessChainContextProvider {
    fn genesis_hash() -> Hash32;
    fn pallet_instance_id() -> u8;
}

pub trait VerifiableRandomness {
    fn request(
        domain: Hash32,
        commitment: Hash32,
        immutable_config_hash: Hash32,
        min_epoch: u64,
    ) -> Result<RandomnessRequestId, DispatchError>;

    fn output(request_id: RandomnessRequestId) -> Option<(u64, Hash32, Hash32)>;

    fn timed_out(request_id: RandomnessRequestId) -> bool;

    fn current_mode() -> RandomnessMode {
        RandomnessMode::Disabled
    }

    /// Realm- and provenance-bound request API. Its fail-closed default keeps
    /// legacy providers source-compatible without making them Production-safe.
    fn request_for(
        _economic_realm: EconomicRealm,
        _expected_provenance: RandomnessMode,
        _domain: Hash32,
        _commitment: Hash32,
        _immutable_config_hash: Hash32,
        _min_epoch: u64,
    ) -> Result<RandomnessRequestId, DispatchError> {
        Err(DispatchError::Other(
            "realm-bound randomness provider unavailable",
        ))
    }

    /// Return an output only when both its economic realm and cryptographic
    /// provenance match the consumer's immutable expectation.
    fn output_for(
        _request_id: RandomnessRequestId,
        _expected_realm: EconomicRealm,
        _expected_provenance: RandomnessMode,
    ) -> Option<RealmBoundRandomnessOutput> {
        None
    }

    /// Production policies must remain fail-closed unless a reviewed,
    /// currently active DrandQuicknet checkpoint is fresh.
    fn production_ready() -> bool {
        false
    }
}

impl VerifiableRandomness for () {
    fn request(
        _domain: Hash32,
        _commitment: Hash32,
        _immutable_config_hash: Hash32,
        _min_epoch: u64,
    ) -> Result<RandomnessRequestId, DispatchError> {
        Err(DispatchError::Other("randomness provider unavailable"))
    }

    fn output(_request_id: RandomnessRequestId) -> Option<(u64, Hash32, Hash32)> {
        None
    }

    fn timed_out(_request_id: RandomnessRequestId) -> bool {
        false
    }

    fn production_ready() -> bool {
        false
    }
}

#[frame_support::pallet]
pub mod pallet {
    use super::*;
    use crate::weights::WeightInfo;
    use frame_system::pallet_prelude::*;
    use sp_runtime::traits::Saturating;
    use sp_std::vec::Vec;

    const STORAGE_VERSION: StorageVersion = StorageVersion::new(1);
    const ALPHA_DOMAIN: &[u8] = b"ETERRA_DETERMINISTIC_ALPHA_RANDOMNESS_V1";

    type RequestOf<T> = RandomnessRequest<BlockNumberFor<T>>;
    type OutputOf<T> = VerifiedRandomnessOutput<BlockNumberFor<T>>;
    type SignatureOf<T> = BoundedVec<u8, <T as Config>::MaxSignatureBytes>;

    #[pallet::config]
    pub trait Config: frame_system::Config {
        type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;
        type AdminOrigin: EnsureOrigin<Self::RuntimeOrigin>;
        type DrandVerifier: DrandProofVerifier;
        type ChainContext: RandomnessChainContextProvider;
        #[pallet::constant]
        type MinFutureEpochs: Get<u64>;
        #[pallet::constant]
        type MinAlphaDelayBlocks: Get<BlockNumberFor<Self>>;
        #[pallet::constant]
        type RequestTimeoutBlocks: Get<BlockNumberFor<Self>>;
        #[pallet::constant]
        type BeaconStaleAfterBlocks: Get<BlockNumberFor<Self>>;
        /// Maximum verified-round age accepted for a checkpoint. Quicknet
        /// emits every three seconds; production uses ten rounds (30 seconds).
        #[pallet::constant]
        type MaxCheckpointLagRounds: Get<u64>;
        /// Consensus time used to bind proofs to quicknet genesis and period.
        /// Relayer submission time is never treated as beacon freshness.
        type UnixTime: UnixTime;
        #[pallet::constant]
        type MaxSignatureBytes: Get<u32>;
        type WeightInfo: WeightInfo;
    }

    #[pallet::pallet]
    #[pallet::storage_version(STORAGE_VERSION)]
    pub struct Pallet<T>(_);

    #[pallet::storage]
    #[pallet::getter(fn mode)]
    pub type CurrentMode<T> = StorageValue<_, RandomnessMode, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn cryptography_review_approved)]
    pub type CryptographyReviewApproved<T> = StorageValue<_, bool, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn next_request_nonce)]
    pub type NextRequestNonce<T> = StorageValue<_, u64, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn request_by_id)]
    pub type Requests<T: Config> =
        StorageMap<_, Blake2_128Concat, RandomnessRequestId, RequestOf<T>, OptionQuery>;

    #[pallet::storage]
    #[pallet::getter(fn output_by_id)]
    pub type Outputs<T: Config> =
        StorageMap<_, Blake2_128Concat, RandomnessRequestId, OutputOf<T>, OptionQuery>;

    #[pallet::storage]
    #[pallet::getter(fn proof_signature)]
    pub type ProofSignatures<T: Config> =
        StorageMap<_, Blake2_128Concat, RandomnessRequestId, SignatureOf<T>, OptionQuery>;

    #[pallet::storage]
    #[pallet::getter(fn latest_verified_round)]
    pub type LatestVerifiedRound<T> = StorageValue<_, u64, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn latest_verified_at)]
    pub type LatestVerifiedAt<T: Config> = StorageValue<_, BlockNumberFor<T>, OptionQuery>;

    #[pallet::storage]
    #[pallet::getter(fn latest_verified_proof_hash)]
    pub type LatestVerifiedProofHash<T> = StorageValue<_, Hash32, OptionQuery>;

    #[pallet::storage]
    #[pallet::getter(fn request_context)]
    pub type RequestContexts<T> =
        StorageMap<_, Blake2_128Concat, RandomnessRequestId, RandomnessRequestContext, OptionQuery>;

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        RandomnessModeChanged {
            mode: RandomnessMode,
        },
        CryptographyReviewStatusChanged {
            approved: bool,
        },
        RandomnessRequested {
            request_id: RandomnessRequestId,
            domain: Hash32,
            exact_epoch: u64,
            mode: RandomnessMode,
            timeout_at: BlockNumberFor<T>,
        },
        RandomnessFinalized {
            request_id: RandomnessRequestId,
            epoch: u64,
            output: Hash32,
            proof_hash: Hash32,
            deterministic_alpha: bool,
        },
        RandomnessTimedOut {
            request_id: RandomnessRequestId,
        },
        DrandCheckpointUpdated {
            round: u64,
            output: Hash32,
            proof_hash: Hash32,
        },
        /// Appended V2 event; the original request event and its codec index
        /// remain unchanged for existing indexers.
        RandomnessRequestContextBound {
            request_id: RandomnessRequestId,
            economic_realm: EconomicRealm,
            expected_provenance: RandomnessMode,
            eterra_genesis_hash: Hash32,
            pallet_instance_id: u8,
            provider_chain_hash: Hash32,
        },
    }

    #[pallet::error]
    pub enum Error<T> {
        RandomnessDisabled,
        CryptographyReviewRequired,
        BeaconStale,
        RequestNotFound,
        RequestNotPending,
        TooEarly,
        NotTimedOut,
        WrongMode,
        WrongRound,
        InvalidDrandProof,
        SignatureTooLong,
        RequestIdExhausted,
        BeaconNotBootstrapped,
        BeaconRoundNotMonotonic,
        BeaconRoundOverflow,
        BeaconClockUnavailable,
        BeaconRoundInFuture,
        ProductionRequiresDrandQuicknet,
        ProvenanceMismatch,
    }

    #[pallet::hooks]
    impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
        fn on_runtime_upgrade() -> Weight {
            let on_chain = StorageVersion::get::<Pallet<T>>();
            if on_chain < STORAGE_VERSION {
                STORAGE_VERSION.put::<Pallet<T>>();
                T::DbWeight::get().reads_writes(1, 1)
            } else {
                T::DbWeight::get().reads(1)
            }
        }
    }

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        #[pallet::call_index(0)]
        #[pallet::weight(T::WeightInfo::set_mode())]
        pub fn set_mode(origin: OriginFor<T>, mode: RandomnessMode) -> DispatchResult {
            T::AdminOrigin::ensure_origin(origin)?;
            if mode == RandomnessMode::DrandQuicknet {
                ensure!(
                    CryptographyReviewApproved::<T>::get(),
                    Error::<T>::CryptographyReviewRequired
                );
                Self::ensure_beacon_fresh()?;
            }
            CurrentMode::<T>::put(mode);
            Self::deposit_event(Event::RandomnessModeChanged { mode });
            Ok(())
        }

        #[pallet::call_index(1)]
        #[pallet::weight(T::WeightInfo::set_mode())]
        pub fn set_cryptography_review_status(
            origin: OriginFor<T>,
            approved: bool,
        ) -> DispatchResult {
            T::AdminOrigin::ensure_origin(origin)?;
            CryptographyReviewApproved::<T>::put(approved);
            if !approved && CurrentMode::<T>::get() == RandomnessMode::DrandQuicknet {
                CurrentMode::<T>::put(RandomnessMode::Disabled);
                Self::deposit_event(Event::RandomnessModeChanged {
                    mode: RandomnessMode::Disabled,
                });
            }
            Self::deposit_event(Event::CryptographyReviewStatusChanged { approved });
            Ok(())
        }

        /// Private-alpha operator helper. Economic policies must reject this mode.
        #[pallet::call_index(2)]
        #[pallet::weight(T::WeightInfo::request())]
        pub fn request_alpha_fixture(
            origin: OriginFor<T>,
            domain: Hash32,
            commitment: Hash32,
            immutable_config_hash: Hash32,
            min_epoch: u64,
        ) -> DispatchResult {
            T::AdminOrigin::ensure_origin(origin)?;
            Self::do_request(domain, commitment, immutable_config_hash, min_epoch)?;
            Ok(())
        }

        #[pallet::call_index(3)]
        #[pallet::weight(T::WeightInfo::submit_drand_quicknet(raw_signature.len() as u32))]
        pub fn submit_drand_quicknet(
            origin: OriginFor<T>,
            request_id: RandomnessRequestId,
            round: u64,
            raw_signature: Vec<u8>,
        ) -> DispatchResult {
            let _ = ensure_signed(origin)?;
            let bounded: SignatureOf<T> = raw_signature
                .try_into()
                .map_err(|_| Error::<T>::SignatureTooLong)?;
            Requests::<T>::try_mutate(request_id, |maybe| -> DispatchResult {
                let request = maybe.as_mut().ok_or(Error::<T>::RequestNotFound)?;
                ensure!(
                    request.status == RequestStatus::Pending,
                    Error::<T>::RequestNotPending
                );
                ensure!(
                    request.mode == RandomnessMode::DrandQuicknet,
                    Error::<T>::WrongMode
                );
                // The request snapshots its verifier mode. Later global pauses
                // stop new requests but cannot strand an already committed
                // exact-round Drand request.
                ensure!(request.exact_epoch == round, Error::<T>::WrongRound);
                let now = frame_system::Pallet::<T>::block_number();
                ensure!(now >= request.not_before, Error::<T>::TooEarly);
                ensure!(now < request.timeout_at, Error::<T>::NotTimedOut);
                ensure!(
                    round <= Self::current_drand_round()?,
                    Error::<T>::BeaconRoundInFuture
                );
                let output = T::DrandVerifier::verify_quicknet(
                    &DRAND_QUICKNET_CHAIN_HASH,
                    round,
                    bounded.as_slice(),
                )
                .ok_or(Error::<T>::InvalidDrandProof)?;
                let proof_hash = sp_io::hashing::blake2_256(bounded.as_slice());
                request.status = RequestStatus::Finalized;
                Outputs::<T>::insert(
                    request_id,
                    VerifiedRandomnessOutput {
                        epoch: round,
                        output,
                        proof_hash,
                        finalized_at: now,
                        deterministic_alpha: false,
                    },
                );
                ProofSignatures::<T>::insert(request_id, bounded);
                // An older in-flight request may legitimately finalize after a
                // newer one. It proves that request, but must never move or
                // refresh the chain-wide beacon checkpoint backwards.
                if round > LatestVerifiedRound::<T>::get() {
                    LatestVerifiedRound::<T>::put(round);
                    LatestVerifiedAt::<T>::put(now);
                    LatestVerifiedProofHash::<T>::put(proof_hash);
                }
                Self::deposit_event(Event::RandomnessFinalized {
                    request_id,
                    epoch: round,
                    output,
                    proof_hash,
                    deterministic_alpha: false,
                });
                Ok(())
            })
        }

        #[pallet::call_index(4)]
        #[pallet::weight(T::WeightInfo::finalize_alpha())]
        pub fn finalize_alpha(
            origin: OriginFor<T>,
            request_id: RandomnessRequestId,
        ) -> DispatchResult {
            let _ = ensure_signed(origin)?;
            ensure!(
                CurrentMode::<T>::get() == RandomnessMode::DeterministicPrivateAlpha,
                Error::<T>::WrongMode
            );
            Requests::<T>::try_mutate(request_id, |maybe| -> DispatchResult {
                let request = maybe.as_mut().ok_or(Error::<T>::RequestNotFound)?;
                ensure!(
                    request.status == RequestStatus::Pending,
                    Error::<T>::RequestNotPending
                );
                ensure!(
                    request.mode == RandomnessMode::DeterministicPrivateAlpha,
                    Error::<T>::WrongMode
                );
                let now = frame_system::Pallet::<T>::block_number();
                ensure!(now >= request.not_before, Error::<T>::TooEarly);
                ensure!(now < request.timeout_at, Error::<T>::NotTimedOut);
                let payload = (
                    ALPHA_DOMAIN,
                    request_id,
                    request.domain,
                    request.commitment,
                    request.immutable_config_hash,
                    request.exact_epoch,
                )
                    .encode();
                let output = sp_io::hashing::blake2_256(&payload);
                let proof_hash = sp_io::hashing::blake2_256(&(b"ALPHA_ONLY", &payload).encode());
                request.status = RequestStatus::Finalized;
                Outputs::<T>::insert(
                    request_id,
                    VerifiedRandomnessOutput {
                        epoch: request.exact_epoch,
                        output,
                        proof_hash,
                        finalized_at: now,
                        deterministic_alpha: true,
                    },
                );
                Self::deposit_event(Event::RandomnessFinalized {
                    request_id,
                    epoch: request.exact_epoch,
                    output,
                    proof_hash,
                    deterministic_alpha: true,
                });
                Ok(())
            })
        }

        #[pallet::call_index(5)]
        #[pallet::weight(T::WeightInfo::timeout())]
        pub fn mark_timed_out(
            origin: OriginFor<T>,
            request_id: RandomnessRequestId,
        ) -> DispatchResult {
            let _ = ensure_signed(origin)?;
            Requests::<T>::try_mutate(request_id, |maybe| -> DispatchResult {
                let request = maybe.as_mut().ok_or(Error::<T>::RequestNotFound)?;
                ensure!(
                    request.status == RequestStatus::Pending,
                    Error::<T>::RequestNotPending
                );
                ensure!(
                    frame_system::Pallet::<T>::block_number() >= request.timeout_at,
                    Error::<T>::NotTimedOut
                );
                request.status = RequestStatus::TimedOut;
                Self::deposit_event(Event::RandomnessTimedOut { request_id });
                Ok(())
            })
        }

        /// Verify and pin a current drand round before enabling production
        /// randomness, and periodically advance it when no request proof has
        /// refreshed the checkpoint. This call never produces gameplay
        /// randomness by itself.
        #[pallet::call_index(6)]
        #[pallet::weight(T::WeightInfo::submit_drand_checkpoint(raw_signature.len() as u32))]
        pub fn submit_drand_checkpoint(
            origin: OriginFor<T>,
            round: u64,
            raw_signature: Vec<u8>,
        ) -> DispatchResult {
            let _ = ensure_signed(origin)?;
            ensure!(
                CryptographyReviewApproved::<T>::get(),
                Error::<T>::CryptographyReviewRequired
            );
            ensure!(
                round > LatestVerifiedRound::<T>::get(),
                Error::<T>::BeaconRoundNotMonotonic
            );
            let current_round = Self::current_drand_round()?;
            ensure!(round <= current_round, Error::<T>::BeaconRoundInFuture);
            ensure!(
                current_round.saturating_sub(round) <= T::MaxCheckpointLagRounds::get(),
                Error::<T>::BeaconStale
            );
            let bounded: SignatureOf<T> = raw_signature
                .try_into()
                .map_err(|_| Error::<T>::SignatureTooLong)?;
            let output = T::DrandVerifier::verify_quicknet(
                &DRAND_QUICKNET_CHAIN_HASH,
                round,
                bounded.as_slice(),
            )
            .ok_or(Error::<T>::InvalidDrandProof)?;
            let now = frame_system::Pallet::<T>::block_number();
            let proof_hash = sp_io::hashing::blake2_256(bounded.as_slice());
            LatestVerifiedRound::<T>::put(round);
            LatestVerifiedAt::<T>::put(now);
            LatestVerifiedProofHash::<T>::put(proof_hash);
            Self::deposit_event(Event::DrandCheckpointUpdated {
                round,
                output,
                proof_hash,
            });
            Ok(())
        }
    }

    impl<T: Config> Pallet<T> {
        pub fn current_drand_round() -> Result<u64, DispatchError> {
            round_at_unix_seconds(T::UnixTime::now().as_secs())
                .ok_or(Error::<T>::BeaconClockUnavailable.into())
        }

        fn ensure_beacon_fresh() -> DispatchResult {
            let last = LatestVerifiedAt::<T>::get().ok_or(Error::<T>::BeaconNotBootstrapped)?;
            let now = frame_system::Pallet::<T>::block_number();
            ensure!(
                now <= last.saturating_add(T::BeaconStaleAfterBlocks::get()),
                Error::<T>::BeaconStale
            );
            ensure!(
                LatestVerifiedRound::<T>::get() > 0
                    && LatestVerifiedProofHash::<T>::get().is_some(),
                Error::<T>::BeaconNotBootstrapped
            );
            let current_round = Self::current_drand_round()?;
            let checkpoint_round = LatestVerifiedRound::<T>::get();
            ensure!(
                checkpoint_round <= current_round,
                Error::<T>::BeaconRoundInFuture
            );
            ensure!(
                current_round.saturating_sub(checkpoint_round) <= T::MaxCheckpointLagRounds::get(),
                Error::<T>::BeaconStale
            );
            Ok(())
        }

        pub fn do_request(
            domain: Hash32,
            commitment: Hash32,
            immutable_config_hash: Hash32,
            min_epoch: u64,
        ) -> Result<RandomnessRequestId, DispatchError> {
            let mode = CurrentMode::<T>::get();
            Self::do_request_for(
                EconomicRealm::Training,
                mode,
                domain,
                commitment,
                immutable_config_hash,
                min_epoch,
            )
        }

        pub fn do_request_for(
            economic_realm: EconomicRealm,
            expected_provenance: RandomnessMode,
            domain: Hash32,
            commitment: Hash32,
            immutable_config_hash: Hash32,
            min_epoch: u64,
        ) -> Result<RandomnessRequestId, DispatchError> {
            ensure!(
                expected_provenance != RandomnessMode::Disabled,
                Error::<T>::RandomnessDisabled
            );
            if economic_realm == EconomicRealm::Production {
                ensure!(
                    expected_provenance == RandomnessMode::DrandQuicknet,
                    Error::<T>::ProductionRequiresDrandQuicknet
                );
            }
            let mode = CurrentMode::<T>::get();
            ensure!(
                mode != RandomnessMode::Disabled,
                Error::<T>::RandomnessDisabled
            );
            ensure!(mode == expected_provenance, Error::<T>::ProvenanceMismatch);
            if mode == RandomnessMode::DrandQuicknet {
                ensure!(
                    CryptographyReviewApproved::<T>::get(),
                    Error::<T>::CryptographyReviewRequired
                );
                Self::ensure_beacon_fresh()?;
            }
            let nonce = NextRequestNonce::<T>::get();
            let next = nonce.checked_add(1).ok_or(Error::<T>::RequestIdExhausted)?;
            let now = frame_system::Pallet::<T>::block_number();
            let clock_epoch = if mode == RandomnessMode::DrandQuicknet {
                Self::current_drand_round()?
            } else {
                LatestVerifiedRound::<T>::get()
            };
            let base_epoch = clock_epoch
                .checked_add(T::MinFutureEpochs::get())
                .ok_or(Error::<T>::BeaconRoundOverflow)?;
            let exact_epoch = min_epoch.max(base_epoch);
            let eterra_genesis_hash = T::ChainContext::genesis_hash();
            let pallet_instance_id = T::ChainContext::pallet_instance_id();
            let provider_chain_hash = if mode == RandomnessMode::DrandQuicknet {
                DRAND_QUICKNET_CHAIN_HASH
            } else {
                [0; 32]
            };
            let request_id = sp_io::hashing::blake2_256(
                &(
                    b"ETERRA_NEXUS_V2",
                    b"ETERRA_RANDOMNESS_REQUEST_V2",
                    eterra_genesis_hash,
                    pallet_instance_id,
                    economic_realm,
                    expected_provenance,
                    provider_chain_hash,
                    nonce,
                    domain,
                    commitment,
                    immutable_config_hash,
                )
                    .encode(),
            );
            let not_before = now.saturating_add(T::MinAlphaDelayBlocks::get());
            let timeout_at = now.saturating_add(T::RequestTimeoutBlocks::get());
            let request = RandomnessRequest {
                request_id,
                domain,
                commitment,
                immutable_config_hash,
                exact_epoch,
                requested_at: now,
                not_before,
                timeout_at,
                mode,
                status: RequestStatus::Pending,
            };
            NextRequestNonce::<T>::put(next);
            Requests::<T>::insert(request_id, request);
            RequestContexts::<T>::insert(
                request_id,
                RandomnessRequestContext {
                    economic_realm,
                    expected_provenance,
                    eterra_genesis_hash,
                    pallet_instance_id,
                    provider_chain_hash,
                },
            );
            Self::deposit_event(Event::RandomnessRequested {
                request_id,
                domain,
                exact_epoch,
                mode,
                timeout_at,
            });
            Self::deposit_event(Event::RandomnessRequestContextBound {
                request_id,
                economic_realm,
                expected_provenance,
                eterra_genesis_hash,
                pallet_instance_id,
                provider_chain_hash,
            });
            Ok(request_id)
        }
    }

    impl<T: Config> VerifiableRandomness for Pallet<T> {
        fn request(
            domain: Hash32,
            commitment: Hash32,
            immutable_config_hash: Hash32,
            min_epoch: u64,
        ) -> Result<RandomnessRequestId, DispatchError> {
            Self::do_request(domain, commitment, immutable_config_hash, min_epoch)
        }

        fn output(request_id: RandomnessRequestId) -> Option<(u64, Hash32, Hash32)> {
            Outputs::<T>::get(request_id)
                .map(|output| (output.epoch, output.output, output.proof_hash))
        }

        fn timed_out(request_id: RandomnessRequestId) -> bool {
            Requests::<T>::get(request_id)
                .map(|request| request.status == RequestStatus::TimedOut)
                .unwrap_or(false)
        }

        fn current_mode() -> RandomnessMode {
            CurrentMode::<T>::get()
        }

        fn request_for(
            economic_realm: EconomicRealm,
            expected_provenance: RandomnessMode,
            domain: Hash32,
            commitment: Hash32,
            immutable_config_hash: Hash32,
            min_epoch: u64,
        ) -> Result<RandomnessRequestId, DispatchError> {
            Self::do_request_for(
                economic_realm,
                expected_provenance,
                domain,
                commitment,
                immutable_config_hash,
                min_epoch,
            )
        }

        fn output_for(
            request_id: RandomnessRequestId,
            expected_realm: EconomicRealm,
            expected_provenance: RandomnessMode,
        ) -> Option<RealmBoundRandomnessOutput> {
            if expected_provenance == RandomnessMode::Disabled
                || (expected_realm == EconomicRealm::Production
                    && expected_provenance != RandomnessMode::DrandQuicknet)
            {
                return None;
            }
            let output = Outputs::<T>::get(request_id)?;
            let actual_provenance = if output.deterministic_alpha {
                RandomnessMode::DeterministicPrivateAlpha
            } else {
                RandomnessMode::DrandQuicknet
            };
            if actual_provenance != expected_provenance {
                return None;
            }

            if let Some(context) = RequestContexts::<T>::get(request_id) {
                let request = Requests::<T>::get(request_id)?;
                if request.status != RequestStatus::Finalized
                    || request.mode != expected_provenance
                    || context.economic_realm != expected_realm
                    || context.expected_provenance != expected_provenance
                    || (expected_realm == EconomicRealm::Production
                        && context.provider_chain_hash != DRAND_QUICKNET_CHAIN_HASH)
                {
                    return None;
                }
                return Some(RealmBoundRandomnessOutput {
                    epoch: output.epoch,
                    output: output.output,
                    proof_hash: output.proof_hash,
                    economic_realm: context.economic_realm,
                    provenance: actual_provenance,
                    provider_chain_hash: context.provider_chain_hash,
                });
            }

            // Compatibility for pre-context Training fixtures and benchmarks.
            // Production never accepts an output lacking an immutable realm
            // binding.
            if expected_realm == EconomicRealm::Production {
                return None;
            }
            if Requests::<T>::get(request_id)
                .is_some_and(|request| request.mode != expected_provenance)
            {
                return None;
            }
            Some(RealmBoundRandomnessOutput {
                epoch: output.epoch,
                output: output.output,
                proof_hash: output.proof_hash,
                economic_realm: EconomicRealm::Training,
                provenance: actual_provenance,
                provider_chain_hash: if actual_provenance == RandomnessMode::DrandQuicknet {
                    DRAND_QUICKNET_CHAIN_HASH
                } else {
                    [0; 32]
                },
            })
        }

        fn production_ready() -> bool {
            CurrentMode::<T>::get() == RandomnessMode::DrandQuicknet
                && CryptographyReviewApproved::<T>::get()
                && Self::ensure_beacon_fresh().is_ok()
        }
    }
}

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;
