use bincode;
use codec::Encode;
use curv::{
	arithmetic::Converter,
	cryptographic_primitives::hashing::{hash_sha256::HSha256, traits::Hash as CurvHash},
	elliptic::curves::secp256_k1::Secp256k1Point,
	BigInt,
};
use log::{debug, error, info, trace, warn};
use round_based::{IsCritical, Msg, StateMachine};
use std::collections::BTreeMap;

use crate::types::*;

pub use multi_party_ecdsa::protocols::multi_party_ecdsa::gg_2020::{
	party_i::*,
	state_machine::{keygen::*, sign::*},
};

/// DKG State tracker
pub struct DKGState<K, P> {
	pub accepted: bool,
	pub is_epoch_over: bool,
	pub curr_dkg: Option<MultiPartyECDSARounds<K, P>>,
	pub past_dkg: Option<MultiPartyECDSARounds<K, P>>,
}

/// State machine structure for performing Keygen, Offline stage and Sign rounds
pub struct MultiPartyECDSARounds<SignPayloadKey, SignPayload> {
	party_index: u16,
	threshold: u16,
	parties: u16,

	keygen_set_id: KeygenSetId,
	signer_set_id: SignerSetId,
	stage: Stage,

	// Message processing
	pending_keygen_msgs: Vec<DKGKeygenMessage>,
	pending_offline_msgs: Vec<DKGOfflineMessage>,

	// Key generation
	keygen: Option<Keygen>,
	local_key: Option<LocalKey>,

	// Offline stage
	offline_stage: Option<OfflineStage>,
	completed_offline_stage: Option<CompletedOfflineStage>,

	// Signing rounds
	rounds: BTreeMap<SignPayloadKey, DKGRoundTracker<SignPayload>>,
	sign_outgoing_msgs: Vec<DKGVoteMessage<SignPayloadKey>>,
	finished_rounds: Vec<DKGSignedPayload<SignPayloadKey, SignPayload>>,
}

impl<K, P> MultiPartyECDSARounds<K, P>
where
	K: Ord + Encode + Copy + core::fmt::Debug,
	P: Encode + core::fmt::Debug,
{
	/// Public ///

	pub fn new(party_index: u16, threshold: u16, parties: u16) -> Self {
		trace!(target: "webb", "🕸️  Creating new MultiPartyECDSARounds, party_index: {}, threshold: {}, parties: {}", party_index, threshold, parties);

		Self {
			party_index,
			threshold,
			parties,
			keygen_set_id: 0,
			signer_set_id: 0,
			stage: Stage::KeygenReady,
			pending_keygen_msgs: Vec::new(),
			pending_offline_msgs: Vec::new(),
			keygen: None,
			local_key: None,
			offline_stage: None,
			completed_offline_stage: None,
			rounds: BTreeMap::new(),
			sign_outgoing_msgs: Vec::new(),
			finished_rounds: Vec::new(),
		}
	}

	pub fn proceed(&mut self) {
		let finished = match self.stage {
			Stage::Keygen => self.proceed_keygen(),
			Stage::Offline => self.proceed_offline_stage(),
			Stage::ManualReady => self.proceed_vote(),
			_ => false,
		};

		if finished {
			self.advance_stage();
		}
	}

	pub fn get_outgoing_messages(&mut self) -> Vec<DKGMsgPayload<K>> {
		trace!(target: "webb", "🕸️  Get outgoing, stage {:?}", self.stage);

		match self.stage {
			Stage::Keygen => self
				.get_outgoing_messages_keygen()
				.into_iter()
				.map(|msg| DKGMsgPayload::Keygen(msg))
				.collect(),
			Stage::Offline => self
				.get_outgoing_messages_offline_stage()
				.into_iter()
				.map(|msg| DKGMsgPayload::Offline(msg))
				.collect(),
			Stage::ManualReady => self
				.get_outgoing_messages_vote()
				.into_iter()
				.map(|msg| DKGMsgPayload::Vote(msg))
				.collect(),
			_ => vec![],
		}
	}

	pub fn handle_incoming(&mut self, data: DKGMsgPayload<K>) -> Result<(), String> {
		trace!(target: "webb", "🕸️  Handle incoming, stage {:?}", self.stage);

		return match data {
			DKGMsgPayload::Keygen(msg) => {
				// TODO: check keygen_set_id
				if Stage::Keygen == self.stage {
					self.handle_incoming_keygen(msg)
				} else {
					self.pending_keygen_msgs.push(msg);
					Ok(())
				}
			},
			DKGMsgPayload::Offline(msg) => {
				// TODO: check signer_set_id
				if Stage::Offline == self.stage {
					self.handle_incoming_offline_stage(msg)
				} else {
					self.pending_offline_msgs.push(msg);
					Ok(())
				}
			},
			DKGMsgPayload::Vote(msg) =>
				if Stage::ManualReady == self.stage {
					self.handle_incoming_vote(msg)
				} else {
					Ok(())
				},
		}
	}

	pub fn start_keygen(&mut self, keygen_set_id: KeygenSetId) -> Result<(), String> {
		info!(
			target: "webb",
			"🕸️  Starting new DKG w/ party_index {:?}, threshold {:?}, size {:?}",
			self.party_index,
			self.threshold,
			self.parties,
		);
		trace!(target: "webb", "🕸️  Keygen set id: {}", keygen_set_id);

		match Keygen::new(self.party_index, self.threshold, self.parties) {
			Ok(new_keygen) => {
				self.stage = Stage::Keygen;
				self.keygen_set_id = keygen_set_id;
				self.keygen = Some(new_keygen);

				// Processing pending messages
				for msg in std::mem::take(&mut self.pending_keygen_msgs) {
					if let Err(err) = self.handle_incoming_keygen(msg) {
						warn!(target: "webb", "🕸️  Error handling pending keygen msg {}", err.to_string());
					}
					self.proceed_keygen();
				}
				trace!(target: "webb", "🕸️  Handled {} pending keygen messages", self.pending_keygen_msgs.len());
				self.pending_keygen_msgs.clear();

				Ok(())
			},
			Err(err) => Err(err.to_string()),
		}
	}

	pub fn reset_signers(
		&mut self,
		signer_set_id: SignerSetId,
		s_l: Vec<u16>,
	) -> Result<(), String> {
		info!(target: "webb", "🕸️  Resetting singers {:?}", s_l);
		info!(target: "webb", "🕸️  Signer set id {:?}", signer_set_id);

		match self.stage {
			Stage::KeygenReady | Stage::Keygen =>
				Err("Cannot reset signers and start offline stage, Keygen is not complete"
					.to_string()),
			_ =>
				if let Some(local_key_clone) = self.local_key.clone() {
					return match OfflineStage::new(self.party_index, s_l, local_key_clone) {
						Ok(new_offline_stage) => {
							self.stage = Stage::Offline;
							self.signer_set_id = signer_set_id;
							self.offline_stage = Some(new_offline_stage);
							self.completed_offline_stage = None;

							for msg in std::mem::take(&mut self.pending_offline_msgs) {
								if let Err(err) = self.handle_incoming_offline_stage(msg) {
									warn!(target: "webb", "🕸️  Error handling pending offline msg {}", err.to_string());
								}
								self.proceed_offline_stage();
							}
							self.pending_offline_msgs.clear();
							trace!(target: "webb", "🕸️  Handled {} pending offline messages", self.pending_offline_msgs.len());

							Ok(())
						},
						Err(err) => {
							error!("Error creating new offline stage {}", err);
							Err(err.to_string())
						},
					}
				} else {
					Err("No local key present".to_string())
				},
		}
	}

	pub fn vote(&mut self, round_key: K, data: P) -> Result<(), String> {
		if let Some(completed_offline) = self.completed_offline_stage.as_mut() {
			let round = self.rounds.entry(round_key).or_default();
			let hash = HSha256::create_hash(&[&BigInt::from_bytes(&data.encode())]);

			match SignManual::new(hash, completed_offline.clone()) {
				Ok((sign_manual, sig)) => {
					trace!(target: "webb", "🕸️  Creating vote /w key {:?}", &round_key);

					round.sign_manual = Some(sign_manual);
					round.payload = Some(data);

					match bincode::serialize(&sig) {
						Ok(serialized_sig) => {
							let msg = DKGVoteMessage {
								party_ind: self.party_index,
								round_key,
								partial_signature: serialized_sig,
							};
							self.sign_outgoing_msgs.push(msg);
							return Ok(())
						},
						Err(err) => return Err(err.to_string()),
					}
				},
				Err(err) => return Err(err.to_string()),
			}
		}
		Err("Not ready to vote".to_string())
	}

	pub fn is_offline_ready(&self) -> bool {
		Stage::OfflineReady == self.stage
	}

	pub fn is_ready_to_vote(&self) -> bool {
		Stage::ManualReady == self.stage
	}

	pub fn has_finished_rounds(&self) -> bool {
		!self.finished_rounds.is_empty()
	}

	pub fn get_finished_rounds(&mut self) -> Vec<DKGSignedPayload<K, P>> {
		std::mem::take(&mut self.finished_rounds)
	}

	pub fn dkg_params(&self) -> (u16, u16, u16) {
		(self.party_index, self.threshold, self.parties)
	}

	pub fn get_public_key(&self) -> Option<Secp256k1Point> {
		if let Some(offline) = &self.completed_offline_stage {
			Some(offline.public_key().clone())
		} else {
			None
		}
	}
}

impl<K, P> MultiPartyECDSARounds<K, P>
where
	K: Ord + Encode + Copy + core::fmt::Debug,
	P: Encode + core::fmt::Debug,
{
	/// Internal ///

	fn advance_stage(&mut self) {
		self.stage = self.stage.get_next();
		info!(target: "webb", "🕸️  New stage {:?}", self.stage);
	}

	/// Proceed to next step for current Stage

	fn proceed_keygen(&mut self) -> bool {
		trace!(target: "webb", "🕸️  Keygen party {} enter proceed", self.party_index);

		let keygen = self.keygen.as_mut().unwrap();

		if keygen.wants_to_proceed() {
			info!(target: "webb", "🕸️  Keygen party {} wants to proceed", keygen.party_ind());
			trace!(target: "webb", "🕸️  before: {:?}", keygen);
			// TODO, handle asynchronously
			match keygen.proceed() {
				Ok(_) => {
					trace!(target: "webb", "🕸️  after: {:?}", keygen);
				},
				Err(err) => {
					error!(target: "webb", "🕸️  error encountered during proceed: {:?}", err);
				},
			}
		}

		self.try_finish_keygen()
	}

	fn proceed_offline_stage(&mut self) -> bool {
		trace!(target: "webb", "🕸️  OfflineStage party {} enter proceed", self.party_index);

		let offline_stage = self.offline_stage.as_mut().unwrap();

		if offline_stage.wants_to_proceed() {
			info!(target: "webb", "🕸️  OfflineStage party {} wants to proceed", offline_stage.party_ind());
			trace!(target: "webb", "🕸️  before: {:?}", offline_stage);
			// TODO, handle asynchronously
			match offline_stage.proceed() {
				Ok(_) => {
					trace!(target: "webb", "🕸️  after: {:?}", offline_stage);
				},
				Err(err) => {
					error!(target: "webb", "🕸️  error encountered during proceed: {:?}", err);
				},
			}
		}

		self.try_finish_offline_stage()
	}

	fn proceed_vote(&mut self) -> bool {
		self.try_finish_vote()
	}

	/// Try finish current Stage

	fn try_finish_keygen(&mut self) -> bool {
		let keygen = self.keygen.as_mut().unwrap();

		if keygen.is_finished() {
			info!(target: "webb", "🕸️  Keygen is finished, extracting output");
			match keygen.pick_output() {
				Some(Ok(k)) => {
					self.local_key = Some(k);
					info!(target: "webb", "🕸️  local share key is extracted");
					return true
				},
				Some(Err(e)) => panic!("Keygen finished with error result {}", e),
				None => panic!("Keygen finished with no result"),
			}
		}
		return false
	}

	fn try_finish_offline_stage(&mut self) -> bool {
		let offline_stage = self.offline_stage.as_mut().unwrap();

		if offline_stage.is_finished() {
			info!(target: "webb", "🕸️  OfflineStage is finished, extracting output");
			match offline_stage.pick_output() {
				Some(Ok(cos)) => {
					self.completed_offline_stage = Some(cos);
					info!(target: "webb", "🕸️  CompletedOfflineStage is extracted");
					return true
				},
				Some(Err(e)) => panic!("OfflineStage finished with error result {}", e),
				None => panic!("OfflineStage finished with no result"),
			}
		}
		return false
	}

	fn try_finish_vote(&mut self) -> bool {
		let mut finished: Vec<K> = Vec::new();

		for (round_key, round) in self.rounds.iter() {
			if round.is_done(self.threshold.into()) {
				finished.push(*round_key);
			}
		}

		trace!(target: "webb", "🕸️  {} Rounds done", finished.len());

		for round_key in finished.iter() {
			if let Some(mut round) = self.rounds.remove(round_key) {
				let sig = round.complete();
				let payload = round.payload;

				if let (Some(payload), Some(sig)) = (payload, sig) {
					match bincode::serialize(&sig) {
						Ok(signature) => {
							let signed_payload =
								DKGSignedPayload { key: *round_key, payload, signature };

							self.finished_rounds.push(signed_payload);

							trace!(target: "webb", "🕸️  Finished round /w key: {:?}", &round_key);
						},
						Err(err) => debug!("Error serializing signature {}", err.to_string()),
					}
				}
			}
		}

		false
	}

	/// Get outgoing messages for current Stage

	fn get_outgoing_messages_keygen(&mut self) -> Vec<DKGKeygenMessage> {
		if let Some(keygen) = self.keygen.as_mut() {
			trace!(target: "webb", "🕸️  Getting outgoing keygen messages");

			if !keygen.message_queue().is_empty() {
				trace!(target: "webb", "🕸️  Outgoing messages, queue len: {}", keygen.message_queue().len());

				let keygen_set_id = self.keygen_set_id;

				let enc_messages = keygen
					.message_queue()
					.into_iter()
					.map(|m| {
						trace!(target: "webb", "🕸️  MPC protocol message {:?}", m);
						let m_ser = bincode::serialize(m).unwrap();
						return DKGKeygenMessage { keygen_set_id, keygen_msg: m_ser }
					})
					.collect::<Vec<DKGKeygenMessage>>();

				keygen.message_queue().clear();
				return enc_messages
			}
		}
		vec![]
	}

	fn get_outgoing_messages_offline_stage(&mut self) -> Vec<DKGOfflineMessage> {
		if let Some(offline_stage) = self.offline_stage.as_mut() {
			trace!(target: "webb", "🕸️  Getting outgoing offline messages");

			if !offline_stage.message_queue().is_empty() {
				trace!(target: "webb", "🕸️  Outgoing messages, queue len: {}", offline_stage.message_queue().len());

				let singer_set_id = self.signer_set_id;

				let enc_messages = offline_stage
					.message_queue()
					.into_iter()
					.map(|m| {
						trace!(target: "webb", "🕸️  MPC protocol message {:?}", *m);
						let m_ser = bincode::serialize(m).unwrap();
						return DKGOfflineMessage {
							signer_set_id: singer_set_id,
							offline_msg: m_ser,
						}
					})
					.collect::<Vec<DKGOfflineMessage>>();

				offline_stage.message_queue().clear();
				return enc_messages
			}
		}
		vec![]
	}

	fn get_outgoing_messages_vote(&mut self) -> Vec<DKGVoteMessage<K>> {
		trace!(target: "webb", "🕸️  Getting outgoing vote messages");
		std::mem::take(&mut self.sign_outgoing_msgs)
	}

	/// Handle incoming messages for current Stage

	fn handle_incoming_keygen(&mut self, data: DKGKeygenMessage) -> Result<(), String> {
		if data.keygen_set_id != self.keygen_set_id {
			return Err("Keygen set ids do not match".to_string())
		}

		if let Some(keygen) = self.keygen.as_mut() {
			trace!(target: "webb", "🕸️  Handle incoming keygen message");
			if data.keygen_msg.is_empty() {
				warn!(
					target: "webb", "🕸️  Got empty message");
				return Ok(())
			}
			let msg: Msg<ProtocolMessage> = match bincode::deserialize(&data.keygen_msg) {
				Ok(msg) => msg,
				Err(err) => {
					error!(target: "webb", "🕸️  Error deserializing msg: {:?}", err);
					return Err("Error deserializing keygen msg".to_string())
				},
			};

			if Some(keygen.party_ind()) != msg.receiver &&
				(msg.receiver.is_some() || msg.sender == keygen.party_ind())
			{
				warn!(target: "webb", "🕸️  Ignore messages sent by self");
				return Ok(())
			}
			trace!(
				target: "webb", "🕸️  Party {} got message from={}, broadcast={}: {:?}",
				keygen.party_ind(),
				msg.sender,
				msg.receiver.is_none(),
				msg.body,
			);
			debug!(target: "webb", "🕸️  State before incoming message processing: {:?}", keygen);
			match keygen.handle_incoming(msg.clone()) {
				Ok(()) => (),
				Err(err) if err.is_critical() => {
					error!(target: "webb", "🕸️  Critical error encountered: {:?}", err);
					return Err("Keygen critical error encountered".to_string())
				},
				Err(err) => {
					error!(target: "webb", "🕸️  Non-critical error encountered: {:?}", err);
				},
			}
			debug!(target: "webb", "🕸️  State after incoming message processing: {:?}", keygen);
		}
		Ok(())
	}

	fn handle_incoming_offline_stage(&mut self, data: DKGOfflineMessage) -> Result<(), String> {
		if data.signer_set_id != self.signer_set_id {
			return Err("Signer set ids do not match".to_string())
		}

		if let Some(offline_stage) = self.offline_stage.as_mut() {
			trace!(target: "webb", "🕸️  Handle incoming offline message");
			if data.offline_msg.is_empty() {
				warn!(
					target: "webb", "🕸️  Got empty message");
				return Ok(())
			}
			let msg: Msg<OfflineProtocolMessage> = match bincode::deserialize(&data.offline_msg) {
				Ok(msg) => msg,
				Err(err) => {
					error!(target: "webb", "🕸️  Error deserializing msg: {:?}", err);
					return Err("Error deserializing offline msg".to_string())
				},
			};

			if Some(offline_stage.party_ind()) != msg.receiver &&
				(msg.receiver.is_some() || msg.sender == offline_stage.party_ind())
			{
				warn!(target: "webb", "🕸️  Ignore messages sent by self");
				return Ok(())
			}
			trace!(
				target: "webb", "🕸️  Party {} got message from={}, broadcast={}: {:?}",
				offline_stage.party_ind(),
				msg.sender,
				msg.receiver.is_none(),
				msg.body,
			);
			debug!(target: "webb", "🕸️  State before incoming message processing: {:?}", offline_stage);
			match offline_stage.handle_incoming(msg.clone()) {
				Ok(()) => (),
				Err(err) if err.is_critical() => {
					error!(target: "webb", "🕸️  Critical error encountered: {:?}", err);
					return Err("Offline critical error encountered".to_string())
				},
				Err(err) => {
					error!(target: "webb", "🕸️  Non-critical error encountered: {:?}", err);
				},
			}
			debug!(target: "webb", "🕸️  State after incoming message processing: {:?}", offline_stage);
		}
		Ok(())
	}

	fn handle_incoming_vote(&mut self, data: DKGVoteMessage<K>) -> Result<(), String> {
		trace!(target: "webb", "🕸️  Handle vote message");

		if data.party_ind == self.party_index {
			warn!(target: "webb", "🕸️  Ignore messages sent by self");
			return Ok(())
		}

		let sig: PartialSignature = match bincode::deserialize(&data.partial_signature) {
			Ok(sig) => sig,
			Err(err) => {
				error!(target: "webb", "🕸️  Error deserializing msg: {:?}", err);
				return Err("Error deserializing vote msg".to_string())
			},
		};

		self.rounds.entry(data.round_key).or_default().add_vote(sig);

		Ok(())
	}
}

struct DKGRoundTracker<Payload> {
	votes: Vec<PartialSignature>,
	sign_manual: Option<SignManual>,
	payload: Option<Payload>,
}

impl<P> Default for DKGRoundTracker<P> {
	fn default() -> Self {
		Self {
			votes: Default::default(),
			sign_manual: Default::default(),
			payload: Default::default(),
		}
	}
}

impl<P> DKGRoundTracker<P> {
	fn add_vote(&mut self, vote: PartialSignature) -> bool {
		// TODO: check for duplicates

		self.votes.push(vote);
		true
	}

	fn is_done(&self, threshold: usize) -> bool {
		self.sign_manual.is_some() && self.votes.len() >= threshold
	}

	fn complete(&mut self) -> Option<SignatureRecid> {
		if let Some(sign_manual) = self.sign_manual.take() {
			return match sign_manual.complete(&self.votes) {
				Ok(sig) => {
					debug!("Obtained complete signature: {}", &sig.recid);
					Some(sig)
				},
				Err(err) => {
					error!("Error signing: {:?}", &err);
					None
				},
			}
		}
		None
	}
}

#[cfg(test)]
mod tests {
	use super::{HSha256, MultiPartyECDSARounds, Stage};
	use bincode;
	use codec::Encode;
	use curv::{arithmetic::Converter, cryptographic_primitives::hashing::traits::Hash, BigInt};
	use multi_party_ecdsa::protocols::multi_party_ecdsa::gg_2020::party_i::verify;

	fn check_all_reached_stage(
		parties: &Vec<MultiPartyECDSARounds<u64, String>>,
		target_stage: Stage,
	) -> bool {
		for party in parties.iter() {
			if party.stage != target_stage {
				return false
			}
		}

		true
	}

	fn check_all_reached_offline_ready(parties: &Vec<MultiPartyECDSARounds<u64, String>>) -> bool {
		check_all_reached_stage(parties, Stage::OfflineReady)
	}

	fn check_all_reached_manual_ready(parties: &Vec<MultiPartyECDSARounds<u64, String>>) -> bool {
		check_all_reached_stage(parties, Stage::ManualReady)
	}

	fn check_all_signatures_ready(parties: &Vec<MultiPartyECDSARounds<u64, String>>) -> bool {
		for party in parties.iter() {
			if !party.has_finished_rounds() {
				return false
			}
		}

		true
	}

	fn check_all_signatures_correct(parties: &mut Vec<MultiPartyECDSARounds<u64, String>>) {
		for party in &mut parties.into_iter() {
			let mut finished_rounds = party.get_finished_rounds();

			if finished_rounds.len() == 1 {
				let finished_round = finished_rounds.remove(0);

				let sig = bincode::deserialize(&finished_round.signature).unwrap();
				let pub_k = party.get_public_key().unwrap();
				let message = HSha256::create_hash(&[&BigInt::from_bytes(&"Webb".encode())]);

				if !verify(&sig, &pub_k, &message).is_ok() {
					panic!("Invalid signature for party {}", party.party_index);
				}
				println!("Party {}; sig: {:?}", party.party_index, &sig);
			} else {
				panic!("No signature extracted")
			}
		}

		for party in &mut parties.into_iter() {
			match party.stage {
				Stage::ManualReady => (),
				_ => panic!("Stage must be ManualReady, but {:?} found", &party.stage),
			}
		}

		println!("All signatures are correct");
	}

	fn run_simulation<C>(parties: &mut Vec<MultiPartyECDSARounds<u64, String>>, stop_condition: C)
	where
		C: Fn(&Vec<MultiPartyECDSARounds<u64, String>>) -> bool,
	{
		println!("Simulation starts");

		let mut msgs_pull = vec![];

		for party in &mut parties.into_iter() {
			party.proceed();

			msgs_pull.append(&mut party.get_outgoing_messages());
		}

		for _i in 1..100 {
			let msgs_pull_frozen = msgs_pull.split_off(0);

			for party in &mut parties.into_iter() {
				for msg_frozen in msgs_pull_frozen.iter() {
					match party.handle_incoming(msg_frozen.clone()) {
						Ok(()) => (),
						Err(err) => panic!("{}", err.to_string()),
					}
				}
				msgs_pull.append(&mut party.get_outgoing_messages());
			}

			for party in &mut parties.into_iter() {
				party.proceed();

				msgs_pull.append(&mut party.get_outgoing_messages());
			}

			if stop_condition(parties) {
				println!("All parties finished");
				return
			}
		}

		panic!("Test failed")
	}

	fn simulate_multi_party(t: u16, n: u16, s_l: Vec<u16>) {
		let mut parties: Vec<MultiPartyECDSARounds<u64, String>> = vec![];

		for i in 1..=n {
			let mut party = MultiPartyECDSARounds::new(i, t, n);
			println!("Starting keygen for party {}, Stage: {:?}", party.party_index, party.stage);
			party.start_keygen(0).unwrap();
			parties.push(party);
		}

		// Running Keygen stage
		println!("Running Keygen");
		run_simulation(&mut parties, check_all_reached_offline_ready);

		// Running Offline stage
		println!("Running Offline");
		let parties_refs = &mut parties;
		for party in parties_refs.into_iter() {
			println!("Resetting signers for party {}, Stage: {:?}", party.party_index, party.stage);
			match party.reset_signers(0, s_l.clone()) {
				Ok(()) => (),
				Err(_err) => (),
			}
		}
		run_simulation(&mut parties, check_all_reached_manual_ready);

		// Running Sign stage
		println!("Running Sign");
		let parties_refs = &mut parties;
		for party in &mut parties_refs.into_iter() {
			println!("Vote for party {}, Stage: {:?}", party.party_index, party.stage);
			party.vote(1, "Webb".to_string()).unwrap();
		}
		run_simulation(&mut parties, check_all_signatures_ready);

		// Extract all signatures and check for correctness
		check_all_signatures_correct(&mut parties);
	}

	#[test]
	fn simulate_multi_party_t2_n3() {
		simulate_multi_party(2, 3, (1..=3).collect());
	}

	#[test]
	fn simulate_multi_party_t9_n10() {
		simulate_multi_party(9, 10, (1..=10).collect());
	}
}