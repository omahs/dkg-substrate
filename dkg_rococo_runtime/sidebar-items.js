window.SIDEBAR_ITEMS = {"constant":[["AVERAGE_ON_INITIALIZE_RATIO","We assume that ~5% of the block weight is consumed by `on_initialize` handlers. This is used to limit the maximal weight of a single extrinsic."],["DAYS",""],["HOURS",""],["MAXIMUM_BLOCK_WEIGHT","We allow for 0.5 of a second of compute with a 12 second average block time."],["MILLISECS_PER_BLOCK","This determines the average expected block time that we are targeting. Blocks will be produced at a minimum duration defined by `SLOT_DURATION`. `SLOT_DURATION` is picked up by `pallet_timestamp` which is in turn picked up by `pallet_aura` to implement `fn slot_duration()`."],["MINUTES",""],["NORMAL_DISPATCH_RATIO","We allow `Normal` extrinsics to fill up the block up to 75%, the rest can be used by `Operational` extrinsics."],["SLOT_DURATION",""],["VERSION","This runtime version."],["WASM_BINARY",""],["WASM_BINARY_BLOATY",""]],"enum":[["BalancesCall","Contains one variant per dispatchable that can be called by an extrinsic."],["DispatchClass","A generalized group of dispatch types."],["Everything","A [`Contains`] implementation that contains every value."],["MultiAddress","A multi-format address wrapper for on-chain accounts."],["OriginCaller",""],["RuntimeCall",""],["RuntimeEvent",""],["SystemCall","Contains one variant per dispatchable that can be called by an extrinsic."],["TimestampCall","Contains one variant per dispatchable that can be called by an extrinsic."]],"fn":[["native_version","The version information used to identify this runtime when compiled natively."]],"macro":[["construct_runtime","Construct a runtime, with the given name and the given pallets."],["match_types","Create a type which implements the `Contains` trait for a particular type with syntax similar to `matches!`."],["parameter_types","Create new implementations of the `Get` trait."]],"mod":[["api",""],["currency","Money matters."],["impls",""],["opaque","Opaque types. These are used by the CLI to instantiate machinery that don’t need to know the specifics of the runtime. They can then be made to be agnostic over specific formats of data like extrinsics, allowing for them to continue syncing the network through upgrades to even the core data structures."],["weights","Expose the auto generated weight files."],["xcm_config",""]],"struct":[["AdjustmentVariable",""],["BasicDeposit",""],["BlockHashCount",""],["ChainIdentifier",""],["CooloffPeriod",""],["CouncilMaxMembers",""],["CouncilMaxProposals",""],["CouncilMotionDuration",""],["CreationFee",""],["DKGAccountId",""],["DecayPercentage",""],["EitherOfDiverse","“OR gate” implementation of `EnsureOrigin` allowing for different `Success` types for `L` and `R`, with them combined using an `Either` type."],["EnactmentPeriod",""],["EqualPrivilegeOnly","Implementation of [`PrivilegeCmp`] that only checks for equal origins."],["ExecutiveBody",""],["ExistentialDeposit",""],["FastTrackVotingPeriod",""],["FieldDeposit",""],["GenesisConfig",""],["IdentityFee","Implementor of `WeightToFee` that maps one unit of weight to one unit of fee."],["IndexDeposit",""],["InstantAllowed",""],["IsInVec","Trivial utility for implementing `Contains`/`OrderedMembership` with a `Vec`."],["LaunchPeriod",""],["MaxAdditionalFields",""],["MaxApprovals",""],["MaxAuthorityProposers",""],["MaxCandidates",""],["MaxExternalProposerAccounts",""],["MaxInvulnerables",""],["MaxLocks",""],["MaxProposals",""],["MaxRegistrars",""],["MaxReserves",""],["MaxResources",""],["MaxSubAccounts",""],["MaxVotes",""],["MaximumSchedulerWeight",""],["MinCandidates",""],["MinVestedTransfer",""],["MinimumDeposit",""],["MinimumMultiplier",""],["MinimumPeriod",""],["NoPreimagePostponement",""],["Offset",""],["OnRuntimeUpgrade",""],["OperationalFeeMultiplier",""],["PalletId","A pallet identifier. These are per pallet and should be stored in a registry somewhere."],["PalletInfo","Provides an implementation of `PalletInfo` to provide information about the pallet setup in the runtime."],["Perbill","A fixed point representation of a number in the range [0, 1]."],["Percent","A fixed point representation of a number in the range [0, 1]."],["Period",""],["Permill","A fixed point representation of a number in the range [0, 1]."],["PotId",""],["PreimageBaseDeposit",""],["PreimageByteDeposit",""],["PreimageMaxSize",""],["ProposalBond",""],["ProposalBondMinimum",""],["ProposalLifetime",""],["RefreshDelay",""],["Runtime",""],["RuntimeApi",""],["RuntimeApiImpl","Implements all runtime apis for the client side."],["RuntimeBlockLength",""],["RuntimeBlockWeights",""],["RuntimeOrigin","The runtime origin type representing the origin of a call."],["SS58Prefix",""],["SessionKeys",""],["SessionLength",""],["SpendPeriod",""],["SubAccountDeposit",""],["TargetBlockFullness",""],["TimeToRestart",""],["TransactionByteFee",""],["TransferFee",""],["TreasuryPalletId",""],["UncleGenerations",""],["UnsignedInterval",""],["UnsignedPriority",""],["UnsignedProposalExpiry",""],["UnvestedFundsAllowedWithdrawReasons",""],["Version",""],["VotingPeriod",""],["Weight",""],["WeightToFee","Handles converting a weight scalar to a fee value, based on the scale and granularity of the node’s balance type."]],"trait":[["BuildStorage","Complex storage builder stuff."],["Currency","Abstraction over a fungible assets system."],["Randomness","A trait that is able to provide randomness."],["StorageValue","A trait for working with macro-generated storage values under the substrate storage API."]],"type":[["AccountId","Some way of identifying an account on the chain. We intentionally make it equivalent to the public key of our transaction signing scheme."],["AccountIndex",""],["Address","The address format for describing accounts."],["AllPallets","All pallets included in the runtime as a nested tuple of types."],["AllPalletsReversedWithSystemFirst","All pallets included in the runtime as a nested tuple of types in reversed order. With the system pallet first."],["AllPalletsWithSystem","All pallets included in the runtime as a nested tuple of types."],["AllPalletsWithSystemReversed","All pallets included in the runtime as a nested tuple of types in reversed order."],["AllPalletsWithoutSystem","All pallets included in the runtime as a nested tuple of types. Excludes the System pallet."],["AllPalletsWithoutSystemReversed","All pallets included in the runtime as a nested tuple of types in reversed order. Excludes the System pallet."],["Aura",""],["AuraConfig",""],["AuraExt",""],["AuraExtConfig",""],["AuraId","An Aura authority identifier using S/R 25519 as its crypto."],["Authorship",""],["Balance","Balance of an account."],["Balances",""],["BalancesConfig",""],["Block","Block type as expected by this runtime."],["BlockId","BlockId type as expected by this runtime."],["BlockNumber","An index to a block."],["CheckedExtrinsic","Extrinsic type that has already been checked."],["CollatorSelection",""],["CollatorSelectionConfig",""],["CollatorSelectionUpdateOrigin",""],["Council",""],["CouncilConfig",""],["CumulusXcm",""],["DKG",""],["DKGConfig",""],["DKGId","Identity of a DKG authority using ECDSA as its crypto."],["DKGProposalHandler",""],["DKGProposals",""],["DKGProposalsConfig",""],["Democracy",""],["DemocracyConfig",""],["DmpQueue",""],["Executive","Executive: handles dispatch to the various modules."],["Hash","A hash of some data used by the chain."],["Header","Block header type as expected by this runtime."],["Identity",""],["Index","Index of a transaction in the chain."],["Indices",""],["IndicesConfig",""],["NegativeImbalance",""],["ParachainInfo",""],["ParachainInfoConfig",""],["ParachainSystem",""],["ParachainSystemConfig",""],["PolkadotXcm",""],["Preimage",""],["RandomnessCollectiveFlip",""],["Reputation","Reputation type"],["Scheduler",""],["Session",""],["SessionConfig",""],["Signature","Alias to 512-bit hash when used in the context of a transaction signature on the chain."],["SignedBlock","A Block signed with a Justification"],["SignedExtra","The SignedExtension to the basic transaction logic."],["SignedPayload","Signed payload"],["Sudo",""],["SudoConfig",""],["System",""],["SystemConfig",""],["Timestamp",""],["TransactionPayment",""],["Treasury",""],["TreasuryConfig",""],["UncheckedExtrinsic","Unchecked extrinsic type as expected by this runtime."],["Utility",""],["Vesting",""],["VestingConfig",""],["XcmpQueue",""]]};