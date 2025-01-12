use crate::{
	mock_blockchain_config::MockBlockchainConfig, transport::*, FinalityNotification,
	MockBlockchainEvent, TestBlock, TestCase,
};
use atomic::Atomic;
use futures::{SinkExt, StreamExt};
use sc_client_api::FinalizeSummary;
use std::{
	collections::{HashMap, VecDeque},
	net::SocketAddr,
	sync::{atomic::Ordering, Arc},
	time::Duration,
};
use tokio::{
	net::{TcpListener, TcpStream},
	sync::{mpsc, Mutex, RwLock},
};
use uuid::Uuid;

pub type PeerId = sc_network::PeerId;

#[derive(Clone)]
pub struct MockBlockchain {
	listener: Arc<Mutex<Option<TcpListener>>>,
	config: MockBlockchainConfig,
	clients: Arc<RwLock<HashMap<PeerId, ConnectedClientState>>>,
	// client sub-tasks communicate with the orchestrator using this sender
	to_orchestrator: mpsc::UnboundedSender<ClientToOrchestratorEvent>,
	// the orchestrator receives updates from its client sub-tasks from this receiver
	orchestrator_rx: Arc<Mutex<Option<mpsc::UnboundedReceiver<ClientToOrchestratorEvent>>>>,
	orchestrator_state: Arc<Atomic<OrchestratorState>>,
}

/// For communicating between the orchestrator task and each spawned client sub-task
#[derive(Debug)]
enum ClientToOrchestratorEvent {
	// Once the client has completed its handshake with the mock blockchain,
	// the client sends this to the orchestrator. The orchestrator will begin
	// running the test cases once "n" peers send this status
	ClientReady,
	TestResult { peer_id: PeerId, trace_id: Uuid, result: TestResult },
}

#[derive(Debug)]
struct TestResult {
	result: Result<(), String>,
}

#[derive(Debug)]
enum OrchestratorToClientEvent {
	// Tells the client subtask to halt
	Halt,
	// Tells the client subtask to send a mock event
	BlockChainEvent { trace_id: Uuid, event: MockBlockchainEvent<TestBlock> },
}

#[derive(Copy, Clone, Default, Debug, Eq, PartialEq)]
enum OrchestratorState {
	#[default]
	// The orchestrator is waiting for n clients to connect
	WaitingForInit,
	// The orchestrator dispatched a round, and, is waiting for the clients
	// to submit a status update back
	AwaitingRoundCompletion,
	// All test cases have been driven to completion
	Complete,
}

struct ConnectedClientState {
	// a map from tracing id => test case. Once the test case passes
	// for the specific client, the test case will be removed from the list
	outstanding_tasks: HashMap<Uuid, crate::TestCase>,
	orchestrator_to_client_subtask: mpsc::UnboundedSender<OrchestratorToClientEvent>,
}

impl MockBlockchain {
	pub async fn new(config: MockBlockchainConfig) -> std::io::Result<Self> {
		let listener = TcpListener::bind(&config.bind).await?;
		let clients = Arc::new(RwLock::new(HashMap::new()));
		let (to_orchestrator, orchestrator_rx) = mpsc::unbounded_channel();
		let orchestrator_state = Arc::new(Atomic::new(Default::default()));

		Ok(Self {
			listener: Arc::new(Mutex::new(Some(listener))),
			config,
			clients,
			orchestrator_state,
			to_orchestrator,
			orchestrator_rx: Arc::new(Mutex::new(Some(orchestrator_rx))),
		})
	}

	pub async fn execute(self) -> std::io::Result<()> {
		let listener = self.listener.lock().await.take().unwrap();
		let this_orchestrator = self.clone();

		// the listener task takes client streams and handles them
		let listener_task = async move {
			while let Ok((stream, addr)) = listener.accept().await {
				let this = self.clone();
				// spawn a sub-task within this listener task
				tokio::task::spawn(this.handle_stream(stream, addr));
			}

			Err::<(), _>(generic_error("Listener died"))
		};

		// the orchestrator task is what sends events to each spawned sub-task within the listener
		// task
		let orchestrator_task = this_orchestrator.orchestrate();

		tokio::try_join!(listener_task, orchestrator_task).map(|_| ())
	}

	/// For debugging purposes, everything will get unwrapped here
	async fn handle_stream(self, stream: TcpStream, addr: SocketAddr) {
		let (mut tx, mut rx) = bind_transport::<TestBlock>(stream);
		// begin handshake process
		let handshake_packet = ProtocolPacket::InitialHandshake;
		tx.send(handshake_packet).await.unwrap();
		let response = rx.next().await.unwrap();

		if let ProtocolPacket::InitialHandshakeResponse { peer_id } = response {
			log::info!(target: "dkg", "Received handshake response from peer {peer_id:?} = {addr:?}");
			let mut write = self.clients.write().await;

			// create a channel for allowing the orchestrator to send this sub-task commands
			let (orchestrator_to_this_task, mut orchestrator_rx) = mpsc::unbounded_channel();
			let state = ConnectedClientState {
				outstanding_tasks: Default::default(),
				orchestrator_to_client_subtask: orchestrator_to_this_task,
			};

			if write.insert(peer_id, state).is_some() {
				// when simulating disconnects, this may happen
				log::warn!(target: "dkg", "Inserted peer {peer_id:?} into the clients map, but, overwrote a previous value")
			}

			std::mem::drop(write);

			// Tell the orchestrator we have established a connection with the client
			self.to_orchestrator.send(ClientToOrchestratorEvent::ClientReady).unwrap();

			let peer_id = &peer_id;

			// this subtask handles passing messages from the DKG client to the orchestrator
			// for tallying results
			let fwd_orchestrator = async move {
				while let Some(packet) = rx.next().await {
					match packet {
						pkt @ ProtocolPacket::InitialHandshake |
						pkt @ ProtocolPacket::InitialHandshakeResponse { .. } |
						pkt @ ProtocolPacket::BlockchainToClient { .. } |
						pkt @ ProtocolPacket::Halt => {
							panic!("Received invalid packet {pkt:?} inside to_orchestrator for {peer_id:?}")
						},
						ProtocolPacket::ClientToBlockchain { event } => {
							let trace_id = event.trace_id;
							let result = TestResult { result: event.result };
							self.to_orchestrator
								.send(ClientToOrchestratorEvent::TestResult {
									peer_id: *peer_id,
									trace_id,
									result,
								})
								.unwrap();
						},
					}
				}
			};

			// this subtask handles receiving commands from the orhcestrator and potentially
			// sending testcases to the DKG clients
			let from_orchestrator = async move {
				while let Some(orchestrator_command) = orchestrator_rx.recv().await {
					match orchestrator_command {
						OrchestratorToClientEvent::Halt => {
							log::info!(target: "dkg", "Peer {peer_id:?} has been requested to halt");
							// tell the subscribing client to shutdown the DKG
							tx.send(ProtocolPacket::Halt).await.unwrap();
							return
						},
						OrchestratorToClientEvent::BlockChainEvent { trace_id, event } => {
							tx.send(ProtocolPacket::BlockchainToClient { trace_id, event })
								.await
								.unwrap();
						},
					}
				}
			};

			let _ = tokio::join!(fwd_orchestrator, from_orchestrator);

			log::warn!(target: "dkg", "Communications between orchestrator and DKG client for peer {peer_id:?} died")
		} else {
			panic!("Invalid first packet received from peer")
		}
	}

	async fn orchestrate(self) -> std::io::Result<()> {
		let mut test_cases = self.generate_test_cases();
		let mut client_to_orchestrator_rx = self.orchestrator_rx.lock().await.take().unwrap();
		let round_id = &mut 0;
		let mut current_round_completed_count = 0;

		let cl = self.clients.clone();
		let state = self.orchestrator_state.clone();

		//spawn the watcher task
		tokio::task::spawn(async move {
			let mut interval = tokio::time::interval(Duration::from_secs(5));
			loop {
				interval.tick().await;
				if state.load(Ordering::SeqCst) != OrchestratorState::AwaitingRoundCompletion {
					continue
				}

				let clients = cl.read().await;
				for (id, client) in clients.iter() {
					if !client.outstanding_tasks.is_empty() {
						log::warn!(target: "dkg", "Client {id:?} has {tasks:?} outstanding task(s)", tasks = client.outstanding_tasks.len());
					}
				}
			}
		});

		while let Some(client_update) = client_to_orchestrator_rx.recv().await {
			match self.orchestrator_state.load(Ordering::SeqCst) {
				o_state @ OrchestratorState::WaitingForInit => match client_update {
					ClientToOrchestratorEvent::ClientReady => {
						let clients = self.clients.read().await;
						// NOTE: the client automatically puts its handle inside this map. We do not
						// have to here
						if clients.len() == self.config.n_clients {
							// we are ready to begin testing rounds
							std::mem::drop(clients);
							self.orchestrator_begin_next_round(&mut test_cases, round_id).await;
						}
					},

					c_update => log_invalid_signal(&o_state, &c_update),
				},

				o_state @ OrchestratorState::AwaitingRoundCompletion => {
					match &client_update {
						ClientToOrchestratorEvent::ClientReady =>
							log_invalid_signal(&o_state, &client_update),
						ClientToOrchestratorEvent::TestResult { peer_id, trace_id, result } => {
							let mut clients = self.clients.write().await;
							let client = clients.get_mut(peer_id).unwrap();
							if let Err(err) = &result.result {
								log::error!(target: "dkg", "Peer {peer_id:?} unsuccessfully completed test {trace_id:?}. Reason: {err:?}");
							// do not remove from map. At the end , any remaining tasks will
							// cause the orchestrator to have a nonzero exit code (useful for
							// pipeline testing)
							} else {
								log::info!(target: "dkg", "Peer {peer_id:?} successfully completed test {trace_id:?}");
								// remove from map
								assert!(client.outstanding_tasks.remove(trace_id).is_some());
							}

							// regardless of success, increment completed count for the current
							// round
							current_round_completed_count += 1;
						},
					}

					// at the end, check if the round is complete
					if current_round_completed_count == self.config.n_clients {
						current_round_completed_count = 0; // reset to 0 for next round
						self.orchestrator_begin_next_round(&mut test_cases, round_id).await
					}
				},
				o_state @ OrchestratorState::Complete =>
					log_invalid_signal(&o_state, &client_update),
			}
		}

		Err(generic_error("client_to_orchestrator_tx's all dropped"))
	}

	fn generate_test_cases(&self) -> VecDeque<TestCase> {
		let mut test_cases = VecDeque::new();

		// add all positive cases to the front
		for _ in 0..self.config.positive_cases {
			test_cases.push_back(TestCase::Valid)
		}

		if let Some(error_cases) = &self.config.error_cases {
			// add all error cases to the back
			for error_case in error_cases {
				for _ in 0..error_case.count {
					test_cases.push_back(TestCase::Invalid(error_case.clone()))
				}
			}
		}

		test_cases
	}

	async fn orchestrator_begin_next_round(
		&self,
		test_cases: &mut VecDeque<TestCase>,
		round_number: &mut u64,
	) {
		log::info!(target: "dkg", "[Orchestrator] Running next round!");

		if let Some(next_case) = test_cases.pop_front() {
			for x in (1..=3).rev() {
				log::info!(target: "dkg", "[Orchestrator] Beginning next test in {x}");
				tokio::time::sleep(Duration::from_millis(1000)).await
			}

			self.orchestrator_set_state(OrchestratorState::AwaitingRoundCompletion);
			let trace_id = Uuid::new_v4();
			// phase 1: send finality notifications to each client
			let mut write = self.clients.write().await;
			let next_finality_notification = create_mocked_finality_blockchain_event(*round_number);
			for (_id, client) in write.iter_mut() {
				client.outstanding_tasks.insert(trace_id, next_case.clone());
				// First, send out a MockBlockChainEvent (happens before each round occurs)
				client
					.orchestrator_to_client_subtask
					.send(OrchestratorToClientEvent::BlockChainEvent {
						trace_id,
						event: next_finality_notification.clone(),
					})
					.unwrap();
			}

			// increment the round number
			*round_number += 1;
		} else {
			log::info!(target: "dkg", "Orchestrator has finished running all tests");
			self.orchestrator_set_state(OrchestratorState::Complete);
			let mut exit_code = 0;
			// check to see the final state
			let read = self.clients.read().await;
			for (peer_id, client_state) in &*read {
				let outstanding_tasks = &client_state.outstanding_tasks;
				// the client should have no outstanding tasks if successful
				let success = outstanding_tasks.is_empty();
				if !success {
					exit_code = 1;
					log::info!(target: "dkg", "Peer {peer_id:?} final state FAILURE | Failed tasks: {outstanding_tasks:?}")
				} else {
					log::info!(target: "dkg", "Peer {peer_id:?} SUCCESS!")
				}
				client_state
					.orchestrator_to_client_subtask
					.send(OrchestratorToClientEvent::Halt)
					.unwrap();
			}

			// Give time for the client subtasks to send relevent packets to the DKG clients
			tokio::time::sleep(std::time::Duration::from_millis(500)).await;
			log::info!(target: "dkg", "Orchestrator is exiting with code {exit_code}");
			std::process::exit(exit_code);
		}
	}

	fn orchestrator_set_state(&self, state: OrchestratorState) {
		self.orchestrator_state.store(state, Ordering::SeqCst);
	}
}

fn generic_error<T: Into<String>>(err: T) -> std::io::Error {
	std::io::Error::new(std::io::ErrorKind::Other, err.into())
}

fn log_invalid_signal(o_state: &OrchestratorState, c_update: &ClientToOrchestratorEvent) {
	log::error!(target: "dkg", "Orchestrator state is {o_state:?}, yet, the client's update state is {c_update:?}")
}

// Two fields are used by the DKG: the block number via header.number(), and the hash via
// header.hash(). So long as these values remain unique, the DKG should work as expected
fn create_mocked_finality_blockchain_event(block_number: u64) -> MockBlockchainEvent<TestBlock> {
	let header = sp_runtime::generic::Header::<u64, _>::new_from_number(block_number);
	let mut slice = [0u8; 32];
	slice[..8].copy_from_slice(&block_number.to_be_bytes());

	let hash = sp_runtime::testing::H256::from(slice);
	let summary = FinalizeSummary { header, finalized: vec![hash], stale_heads: vec![] };

	let (tx, _rx) = sc_utils::mpsc::tracing_unbounded("mpsc_finality_notification", 999999);
	let notification = FinalityNotification::<TestBlock>::from_summary(summary, tx);
	MockBlockchainEvent::FinalityNotification { notification }
}
