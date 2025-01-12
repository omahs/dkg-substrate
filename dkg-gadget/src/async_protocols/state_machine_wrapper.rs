// Copyright 2022 Webb Technologies Inc.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use dkg_primitives::types::SessionId;
use multi_party_ecdsa::protocols::multi_party_ecdsa::gg_2020::state_machine::traits::RoundBlame;
use round_based::{Msg, StateMachine};
use sp_runtime::traits::Get;
use std::{collections::HashSet, sync::Arc};

use super::{CurrentRoundBlame, ProtocolType};
use crate::debug_logger::DebugLogger;

pub(crate) struct StateMachineWrapper<
	T: StateMachine,
	MaxProposalLength: Get<u32> + Clone + Send + Sync + std::fmt::Debug + 'static,
> {
	sm: T,
	session_id: SessionId,
	channel_type: ProtocolType<MaxProposalLength>,
	current_round_blame: Arc<tokio::sync::watch::Sender<CurrentRoundBlame>>,
	// stores a list of received messages
	received_messages: HashSet<Vec<u8>>,
	logger: DebugLogger,
}

impl<
		T: StateMachine + RoundBlame,
		MaxProposalLength: Get<u32> + Clone + Send + Sync + std::fmt::Debug + 'static,
	> StateMachineWrapper<T, MaxProposalLength>
{
	pub fn new(
		sm: T,
		session_id: SessionId,
		channel_type: ProtocolType<MaxProposalLength>,
		current_round_blame: Arc<tokio::sync::watch::Sender<CurrentRoundBlame>>,
		logger: DebugLogger,
	) -> Self {
		Self {
			sm,
			session_id,
			channel_type,
			current_round_blame,
			logger,
			received_messages: HashSet::new(),
		}
	}

	fn collect_round_blame(&self) {
		let (unreceived_messages, blamed_parties) = self.round_blame();
		let _ = self
			.current_round_blame
			.send(CurrentRoundBlame { unreceived_messages, blamed_parties });
	}
}

impl<T, MaxProposalLength: Get<u32> + Clone + Send + Sync + std::fmt::Debug + 'static> StateMachine
	for StateMachineWrapper<T, MaxProposalLength>
where
	T: StateMachine + RoundBlame,
	<T as StateMachine>::Err: std::fmt::Debug,
	<T as StateMachine>::MessageBody: serde::Serialize,
{
	type Err = T::Err;
	type Output = T::Output;
	type MessageBody = T::MessageBody;

	fn handle_incoming(&mut self, msg: Msg<Self::MessageBody>) -> Result<(), Self::Err> {
		self.logger.trace(format!(
			"Handling incoming message for {:?} from session={}, round={}, sender={}",
			self.channel_type,
			self.session_id,
			self.current_round(),
			msg.sender
		));

		// Before passing to the state machine, make sure that we haven't already received the same
		// message (this is needed as we use a gossiping protocol to send messages, and we don't
		// want to process the same message twice)
		let msg_serde = bincode2::serialize(&msg).expect("Failed to serialize message");
		if self.received_messages.contains(&msg_serde) {
			self.logger.trace(format!(
				"Already received message for {:?} from session={}, round={}, sender={}",
				self.channel_type,
				self.session_id,
				self.current_round(),
				msg.sender
			));
			return Ok(())
		} else {
			self.received_messages.insert(msg_serde);
		}

		let result = self.sm.handle_incoming(msg);
		if let Some(err) = result.as_ref().err() {
			self.logger.error(format!("StateMachine error: {err:?}"));
		}

		self.collect_round_blame();
		result
	}

	fn message_queue(&mut self) -> &mut Vec<Msg<Self::MessageBody>> {
		if !self.sm.message_queue().is_empty() {
			self.logger.trace(format!(
				"Preparing to drain message queue for {:?} in session={}, round={}, queue size={}",
				self.channel_type,
				self.session_id,
				self.current_round(),
				self.sm.message_queue().len(),
			));
		}
		self.sm.message_queue()
	}

	fn wants_to_proceed(&self) -> bool {
		self.sm.wants_to_proceed()
	}

	fn proceed(&mut self) -> Result<(), Self::Err> {
		self.logger.trace(format!(
			"Trying to proceed: current round ({:?}), waiting for msgs from parties: ({:?})",
			self.current_round(),
			self.round_blame(),
		));
		let result = self.sm.proceed();
		self.logger.trace(format!(
			"Proceeded through SM: ({:?}), new current round ({:?}), waiting for msgs from parties: ({:?})",
			self.channel_type,
			self.current_round(),
			self.round_blame(),
		));
		self.collect_round_blame();
		result
	}

	fn round_timeout(&self) -> Option<std::time::Duration> {
		self.sm.round_timeout()
	}

	fn round_timeout_reached(&mut self) -> Self::Err {
		self.sm.round_timeout_reached()
	}

	fn is_finished(&self) -> bool {
		self.sm.is_finished()
	}

	fn pick_output(&mut self) -> Option<Result<Self::Output, Self::Err>> {
		self.sm.pick_output()
	}

	fn current_round(&self) -> u16 {
		self.sm.current_round()
	}

	fn total_rounds(&self) -> Option<u16> {
		self.sm.total_rounds()
	}

	fn party_ind(&self) -> u16 {
		self.sm.party_ind()
	}

	fn parties(&self) -> u16 {
		self.sm.parties()
	}
}

impl<
		T: StateMachine + RoundBlame,
		MaxProposalLength: Get<u32> + Clone + Send + Sync + std::fmt::Debug + 'static,
	> RoundBlame for StateMachineWrapper<T, MaxProposalLength>
{
	fn round_blame(&self) -> (u16, Vec<u16>) {
		self.sm.round_blame()
	}
}
