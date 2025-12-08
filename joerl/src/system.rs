//! Actor system for managing the lifecycle of actors.
//!
//! The ActorSystem is responsible for:
//! - Spawning new actors
//! - Maintaining a registry of active actors
//! - Routing messages between actors
//! - Managing actor lifecycle (links, monitors)

use crate::Pid;
use crate::actor::{Actor, ActorContext};
use crate::error::{ActorError, Result};
use crate::mailbox::{DEFAULT_MAILBOX_CAPACITY, Mailbox, MailboxSender};
use crate::message::{Envelope, EnvelopeContent, ExitReason, Message, MonitorRef, Signal};
use crate::telemetry::{ActorMetrics, LinkMetrics, MessageMetrics, SignalMetrics};
use dashmap::DashMap;
use futures::FutureExt;
use std::collections::HashSet;
use std::panic::AssertUnwindSafe;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::task::JoinHandle;

// Distributed system imports (optional features)
use crate::epmd::EpmdClient;
use std::time::Duration;
use tokio::net::TcpListener;
use tracing::info;

static MONITOR_COUNTER: AtomicU64 = AtomicU64::new(1);

/// Handle to a spawned actor.
///
/// The ActorRef allows you to:
/// - Send messages to the actor
/// - Link to or monitor the actor
/// - Wait for the actor to terminate
pub struct ActorRef {
    pid: Pid,
    system: Arc<ActorSystem>,
    join_handle: Option<JoinHandle<ExitReason>>,
}

impl ActorRef {
    /// Returns the Pid of the actor.
    ///
    /// The Pid can be used to identify the actor, send messages via the system,
    /// or establish links and monitors.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use joerl::{ActorSystem, Actor, ActorContext, Message};
    /// use async_trait::async_trait;
    ///
    /// struct TestActor;
    ///
    /// #[async_trait]
    /// impl Actor for TestActor {
    ///     async fn handle_message(&mut self, _msg: Message, _ctx: &mut ActorContext) {}
    /// }
    ///
    /// # tokio_test::block_on(async {
    /// let system = ActorSystem::new();
    /// let actor_ref = system.spawn(TestActor);
    /// let pid = actor_ref.pid();
    /// println!("Actor PID: {}", pid);
    /// # });
    /// ```
    pub fn pid(&self) -> Pid {
        self.pid
    }

    /// Sends a message to the actor.
    ///
    /// Returns an error if the actor is no longer alive or the mailbox is full.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use joerl::{ActorSystem, ActorRef};
    /// # async fn example(actor_ref: ActorRef) {
    /// actor_ref.send(Box::new("Hello".to_string())).await;
    /// # }
    /// ```
    pub async fn send(&self, msg: Message) -> Result<()> {
        self.system.send(self.pid, msg).await
    }

    /// Attempts to send a message without blocking.
    pub fn try_send(&self, msg: Message) -> Result<()> {
        self.system.try_send(self.pid, msg)
    }

    /// Links this actor to another actor (bidirectional).
    ///
    /// Creates a bidirectional link between two actors. If either actor terminates
    /// abnormally, the other will receive an EXIT signal (and terminate unless
    /// trapping exits).
    ///
    /// In Erlang: `link(Pid)`
    ///
    /// # Arguments
    ///
    /// * `from` - The Pid of the actor to link from
    ///
    /// # Errors
    ///
    /// Returns `ActorError::ActorNotFound` if either actor doesn't exist.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use joerl::{ActorSystem, Actor, ActorContext, Message};
    /// use async_trait::async_trait;
    ///
    /// struct Worker;
    ///
    /// #[async_trait]
    /// impl Actor for Worker {
    ///     async fn handle_message(&mut self, _msg: Message, _ctx: &mut ActorContext) {}
    /// }
    ///
    /// # tokio_test::block_on(async {
    /// let system = ActorSystem::new();
    /// let worker1 = system.spawn(Worker);
    /// let worker2 = system.spawn(Worker);
    ///
    /// // Link worker1 to worker2
    /// worker2.link(worker1.pid()).expect("Failed to link");
    /// // Now if either fails, both will be notified
    /// # });
    /// ```
    pub fn link(&self, from: Pid) -> Result<()> {
        self.system.link(from, self.pid)
    }

    /// Monitors this actor (unidirectional).
    ///
    /// Creates a unidirectional monitor. When this actor terminates, the
    /// monitoring actor receives a DOWN signal but is not affected.
    ///
    /// Returns a MonitorRef that will be included in the DOWN message,
    /// allowing the monitor to identify which actor terminated.
    ///
    /// In Erlang: `monitor(process, Pid)`
    ///
    /// # Arguments
    ///
    /// * `from` - The Pid of the monitoring actor
    ///
    /// # Errors
    ///
    /// Returns `ActorError::ActorNotFound` if either actor doesn't exist.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use joerl::{ActorSystem, Actor, ActorContext, Message, Signal};
    /// use async_trait::async_trait;
    ///
    /// struct Observer;
    ///
    /// #[async_trait]
    /// impl Actor for Observer {
    ///     async fn handle_message(&mut self, _msg: Message, _ctx: &mut ActorContext) {}
    ///     
    ///     async fn handle_signal(&mut self, signal: Signal, _ctx: &mut ActorContext) {
    ///         if let Signal::Down { pid, reason, .. } = signal {
    ///             println!("Monitored actor {} terminated: {}", pid, reason);
    ///         }
    ///     }
    /// }
    ///
    /// struct Worker;
    ///
    /// #[async_trait]
    /// impl Actor for Worker {
    ///     async fn handle_message(&mut self, _msg: Message, _ctx: &mut ActorContext) {}
    /// }
    ///
    /// # tokio_test::block_on(async {
    /// let system = ActorSystem::new();
    /// let observer = system.spawn(Observer);
    /// let worker = system.spawn(Worker);
    ///
    /// // Observer monitors worker
    /// let monitor_ref = worker.monitor(observer.pid()).expect("Failed to monitor");
    /// # });
    /// ```
    pub fn monitor(&self, from: Pid) -> Result<MonitorRef> {
        self.system.monitor(from, self.pid)
    }

    /// Waits for the actor to terminate and returns the exit reason.
    ///
    /// This consumes the ActorRef and blocks until the actor has fully
    /// terminated, returning the reason it stopped.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use joerl::{ActorSystem, Actor, ActorContext, Message, ExitReason};
    /// use async_trait::async_trait;
    ///
    /// struct ShortLived;
    ///
    /// #[async_trait]
    /// impl Actor for ShortLived {
    ///     async fn started(&mut self, ctx: &mut ActorContext) {
    ///         ctx.stop(ExitReason::Normal);
    ///     }
    ///     
    ///     async fn handle_message(&mut self, _msg: Message, _ctx: &mut ActorContext) {}
    /// }
    ///
    /// # async fn example() {
    /// let system = ActorSystem::new();
    /// let actor = system.spawn(ShortLived);
    ///
    /// // Wait for the actor to terminate
    /// let exit_reason = actor.join().await;
    /// assert_eq!(exit_reason, ExitReason::Normal);
    /// # }
    /// ```
    pub async fn join(mut self) -> ExitReason {
        if let Some(handle) = self.join_handle.take() {
            handle
                .await
                .unwrap_or(ExitReason::Panic("Actor task panicked".to_string()))
        } else {
            ExitReason::Normal
        }
    }
}

/// Internal actor entry in the registry.
struct ActorEntry {
    sender: MailboxSender,
    links: HashSet<Pid>,
    monitors: HashSet<(Pid, MonitorRef)>, // (monitoring_pid, ref)
    actor_type: String,                   // Type name for telemetry
    #[cfg(feature = "telemetry")]
    spawn_time: std::time::Instant, // For lifetime tracking
}

/// The actor system runtime.
///
/// This is the main entry point for creating and managing actors.
/// The same ActorSystem handles both local and distributed scenarios seamlessly.
///
/// # Examples
///
/// ## Local-only system
/// ```
/// use joerl::ActorSystem;
///
/// #[tokio::main]
/// async fn main() {
///     let system = ActorSystem::new();
///     // Spawn actors...
/// }
/// ```
///
/// ## Distributed system
/// ```no_run
/// use joerl::ActorSystem;
///
/// #[tokio::main]
/// async fn main() {
///     let system = ActorSystem::new_distributed(
///         "my_node",
///         "127.0.0.1:5000",
///         "127.0.0.1:4369"
///     ).await.expect("Failed to create distributed system");
///     // Use exactly the same API!
/// }
/// ```
pub struct ActorSystem {
    actors: Arc<DashMap<Pid, ActorEntry>>,
    local_node_id: u32, // 0 for local-only, or node hash for distributed

    // Distributed system fields (None for local-only systems)
    node_name: Option<String>,
    listen_address: Option<String>,
    epmd_client: Option<EpmdClient>,
    node_registry: Option<Arc<crate::distributed::NodeRegistry>>,
    _listener_handle: Option<JoinHandle<()>>,
}

impl ActorSystem {
    /// Creates a new local-only actor system.
    ///
    /// This creates a system that only manages actors on the local node.
    /// For distributed systems, use `new_distributed()`.
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            actors: Arc::new(DashMap::new()),
            local_node_id: 0,
            node_name: None,
            listen_address: None,
            epmd_client: None,
            node_registry: None,
            _listener_handle: None,
        })
    }

    /// Creates a new distributed actor system with EPMD integration.
    ///
    /// This enables location-transparent messaging across multiple nodes.
    /// The API is exactly the same as local systems - sending, linking, and
    /// monitoring work transparently whether actors are local or remote.
    ///
    /// # Arguments
    ///
    /// * `node_name` - Unique name for this node (e.g., "worker1")
    /// * `listen_address` - Address to listen for incoming connections (e.g., "127.0.0.1:5000")
    /// * `epmd_address` - Address of EPMD server (e.g., "127.0.0.1:4369")
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use joerl::ActorSystem;
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let system = ActorSystem::new_distributed(
    ///         "my_node",
    ///         "127.0.0.1:5000",
    ///         "127.0.0.1:4369"
    ///     ).await.expect("Failed to create system");
    ///     
    ///     // Use the same API as local systems!
    ///     // let actor = system.spawn(MyActor);
    /// }
    /// ```
    pub async fn new_distributed(
        node_name: impl Into<String>,
        listen_address: impl Into<String>,
        epmd_address: impl Into<String>,
    ) -> Result<Arc<Self>> {
        let node_name = node_name.into();
        let listen_address = listen_address.into();
        let epmd_address = epmd_address.into();

        // Calculate node ID from name
        let node_id = Self::hash_node_name(&node_name);

        // Create EPMD client
        let epmd_client = EpmdClient::new(epmd_address);

        // Extract host and port from listen_address
        let parts: Vec<&str> = listen_address.split(':').collect();
        let host = parts[0].to_string();
        let port: u16 = parts.get(1).and_then(|p| p.parse().ok()).ok_or_else(|| {
            ActorError::other(format!("Invalid listen address: {}", listen_address))
        })?;

        // Register with EPMD
        epmd_client
            .register(&node_name, &host, port)
            .await
            .map_err(|e| ActorError::other(format!("EPMD registration failed: {}", e)))?;
        info!(
            "Registered node {} with EPMD at {}:{} (node_id: {})",
            node_name, host, port, node_id
        );

        // Start keep-alive loop
        epmd_client
            .start_keep_alive_loop(node_name.clone(), Duration::from_secs(20))
            .await;

        // Create node registry
        let node_registry = Arc::new(crate::distributed::NodeRegistry::new());

        // Start TCP listener for incoming connections
        let listener = TcpListener::bind(&listen_address)
            .await
            .map_err(|e| ActorError::other(format!("Failed to bind listener: {}", e)))?;
        info!("Listening for node connections on {}", listen_address);

        // Create the system first
        let system = Arc::new(Self {
            actors: Arc::new(DashMap::new()),
            local_node_id: node_id,
            node_name: Some(node_name.clone()),
            listen_address: Some(listen_address.clone()),
            epmd_client: Some(epmd_client),
            node_registry: Some(Arc::clone(&node_registry)),
            _listener_handle: None,
        });

        // Start accepting connections
        let system_clone = Arc::clone(&system);
        let _listener_handle = tokio::spawn(async move {
            crate::distributed::accept_connections(
                listener,
                system_clone,
                node_name,
                node_id,
                listen_address,
            )
            .await;
        });

        // Note: listener_handle is intentionally not stored in the struct
        // It will run for the lifetime of the program

        Ok(system)
    }

    /// Returns the local node ID (0 for local-only systems).
    pub fn local_node_id(&self) -> u32 {
        self.local_node_id
    }

    /// Returns the EPMD client (if distributed).
    pub(crate) fn epmd_client(&self) -> Option<&EpmdClient> {
        self.epmd_client.as_ref()
    }

    /// Returns the node registry (if distributed).
    pub(crate) fn node_registry(&self) -> Option<&Arc<crate::distributed::NodeRegistry>> {
        self.node_registry.as_ref()
    }

    /// Returns the listen address (if distributed).
    pub(crate) fn listen_address(&self) -> Option<&str> {
        self.listen_address.as_deref()
    }

    /// Hashes a node name to a node ID.
    ///
    /// This is useful when you need to construct Pids for remote actors.
    /// The hash is deterministic - the same node name always produces the same ID.
    pub fn hash_node_name(name: &str) -> u32 {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        name.hash(&mut hasher);
        (hasher.finish() & 0xFFFFFFFF) as u32
    }

    /// Normalizes a Pid for location transparency.
    ///
    /// Converts between local (node==0) and distributed (node==local_node_id) representations.
    /// This makes Pids work transparently regardless of how they were created.
    fn normalize_pid(&self, pid: Pid) -> Pid {
        // If we're local-only, always use node==0
        if self.local_node_id == 0 {
            return Pid::with_node(0, pid.id());
        }

        // If we're distributed:
        // - Convert node==0 to our local_node_id (for outgoing)
        // - Convert node==local_node_id to node==0 (for local lookup)
        match pid.node() {
            0 => Pid::with_node(self.local_node_id, pid.id()),
            n if n == self.local_node_id => Pid::with_node(0, pid.id()),
            _ => pid, // Remote pid, keep as-is
        }
    }

    /// Spawns a new actor with default mailbox capacity.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use joerl::{Actor, ActorSystem, ActorContext, Message};
    /// # use async_trait::async_trait;
    /// # struct MyActor;
    /// # #[async_trait]
    /// # impl Actor for MyActor {
    /// #     async fn handle_message(&mut self, msg: Message, ctx: &mut ActorContext) {}
    /// # }
    /// # async fn example() {
    /// let system = ActorSystem::new();
    /// let actor_ref = system.spawn(MyActor);
    /// # }
    /// ```
    pub fn spawn<A: Actor>(self: &Arc<Self>, actor: A) -> ActorRef {
        self.spawn_with_capacity(actor, DEFAULT_MAILBOX_CAPACITY)
    }

    /// Spawns a boxed actor with default mailbox capacity.
    ///
    /// This is useful for spawning trait objects or when you need dynamic
    /// dispatch for actors.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use joerl::{ActorSystem, Actor, ActorContext, Message};
    /// use async_trait::async_trait;
    ///
    /// struct Worker;
    ///
    /// #[async_trait]
    /// impl Actor for Worker {
    ///     async fn handle_message(&mut self, _msg: Message, _ctx: &mut ActorContext) {}
    /// }
    ///
    /// # tokio_test::block_on(async {
    /// let system = ActorSystem::new();
    /// let boxed_actor: Box<dyn Actor> = Box::new(Worker);
    /// let actor_ref = system.spawn_boxed(boxed_actor);
    /// # });
    /// ```
    pub fn spawn_boxed(self: &Arc<Self>, actor: Box<dyn Actor>) -> ActorRef {
        self.spawn_boxed_with_capacity(actor, DEFAULT_MAILBOX_CAPACITY)
    }

    /// Spawns a new actor with a specific mailbox capacity.
    ///
    /// Allows customization of the mailbox size. Larger mailboxes can buffer
    /// more messages but use more memory. Smaller mailboxes provide stronger
    /// backpressure.
    ///
    /// # Arguments
    ///
    /// * `actor` - The actor to spawn
    /// * `capacity` - The mailbox capacity (number of messages)
    ///
    /// # Examples
    ///
    /// ```rust
    /// use joerl::{ActorSystem, Actor, ActorContext, Message};
    /// use async_trait::async_trait;
    ///
    /// struct HighThroughputActor;
    ///
    /// #[async_trait]
    /// impl Actor for HighThroughputActor {
    ///     async fn handle_message(&mut self, _msg: Message, _ctx: &mut ActorContext) {}
    /// }
    ///
    /// # tokio_test::block_on(async {
    /// let system = ActorSystem::new();
    /// // Spawn with larger mailbox for high-throughput scenarios
    /// let actor_ref = system.spawn_with_capacity(HighThroughputActor, 1000);
    /// # });
    /// ```
    pub fn spawn_with_capacity<A: Actor>(self: &Arc<Self>, actor: A, capacity: usize) -> ActorRef {
        let actor_type = crate::telemetry::actor_type_name::<A>();
        self.spawn_internal(Box::new(actor), capacity, actor_type)
    }

    /// Spawns a boxed actor with a specific mailbox capacity.
    ///
    /// Combines the flexibility of boxed actors with custom mailbox sizing.
    ///
    /// # Arguments
    ///
    /// * `actor` - The boxed actor to spawn
    /// * `capacity` - The mailbox capacity (number of messages)
    ///
    /// # Examples
    ///
    /// ```rust
    /// use joerl::{ActorSystem, Actor, ActorContext, Message};
    /// use async_trait::async_trait;
    ///
    /// struct Worker;
    ///
    /// #[async_trait]
    /// impl Actor for Worker {
    ///     async fn handle_message(&mut self, _msg: Message, _ctx: &mut ActorContext) {}
    /// }
    ///
    /// # tokio_test::block_on(async {
    /// let system = ActorSystem::new();
    /// let boxed: Box<dyn Actor> = Box::new(Worker);
    /// let actor_ref = system.spawn_boxed_with_capacity(boxed, 500);
    /// # });
    /// ```
    pub fn spawn_boxed_with_capacity(
        self: &Arc<Self>,
        actor: Box<dyn Actor>,
        capacity: usize,
    ) -> ActorRef {
        self.spawn_internal(actor, capacity, "boxed")
    }

    /// Internal spawn implementation.
    fn spawn_internal(
        self: &Arc<Self>,
        mut actor: Box<dyn Actor>,
        capacity: usize,
        actor_type: &str,
    ) -> ActorRef {
        // Create Pid with the system's local_node_id (0 for local-only, node_id for distributed)
        let local_pid = Pid::new();
        let pid = if self.local_node_id == 0 {
            local_pid
        } else {
            Pid::with_node(self.local_node_id, local_pid.id())
        };
        let actor_type_owned = actor_type.to_string();
        let (mailbox, sender) = Mailbox::new_with_type(capacity, actor_type_owned.clone());
        let mut ctx = ActorContext::new(pid, mailbox);

        // Set system reference in context
        ctx.set_system(Arc::downgrade(self));

        // Register actor
        let entry = ActorEntry {
            sender: sender.clone(),
            links: HashSet::new(),
            monitors: HashSet::new(),
            actor_type: actor_type_owned.clone(),
            #[cfg(feature = "telemetry")]
            spawn_time: std::time::Instant::now(),
        };
        self.actors.insert(pid, entry);

        // Telemetry: record actor spawn with type
        ActorMetrics::actor_spawned_typed(&actor_type_owned);

        let system = Arc::clone(self);
        let actor_type_for_panic = actor_type_owned.clone();
        let join_handle = tokio::spawn(async move {
            // Wrap the entire actor execution in catch_unwind to handle panics
            let exit_reason = AssertUnwindSafe(async {
                // Call started hook
                actor.started(&mut ctx).await;

                // Main message loop
                let exit_reason = loop {
                    match ctx.recv().await {
                        Some(envelope) => {
                            // Track queue wait time
                            #[cfg(feature = "telemetry")]
                            {
                                let wait_time = envelope.enqueued_at.elapsed().as_secs_f64();
                                MessageMetrics::message_queue_wait(wait_time);
                            }

                            match envelope.content {
                                EnvelopeContent::Message(msg) => {
                                    let _span = MessageMetrics::message_processing_span();
                                    actor.handle_message(msg, &mut ctx).await;
                                    MessageMetrics::message_processed();
                                }
                                EnvelopeContent::Signal(signal) => {
                                    // Telemetry: track signal reception
                                    let signal_type = match &signal {
                                        Signal::Exit { .. } => "exit",
                                        Signal::Down { .. } => "down",
                                        Signal::Stop => "stop",
                                        Signal::Kill => "kill",
                                    };
                                    SignalMetrics::signal_received(signal_type);

                                    // Check if signal will be ignored (trapped)
                                    let is_trapped = if let Signal::Exit { reason, .. } = &signal {
                                        ctx.is_trapping_exits() && reason.is_trappable()
                                    } else {
                                        false
                                    };

                                    if is_trapped {
                                        SignalMetrics::signal_ignored(signal_type);
                                    }

                                    actor.handle_signal(signal, &mut ctx).await;
                                }
                            }
                        }
                        None => {
                            // Mailbox closed, exit normally
                            break ExitReason::Normal;
                        }
                    }

                    // Check if actor should stop
                    if ctx.should_stop() {
                        break ctx.stop_reason().cloned().unwrap_or(ExitReason::Normal);
                    }
                };

                // Call stopped hook
                actor.stopped(&exit_reason, &mut ctx).await;

                exit_reason
            })
            .catch_unwind()
            .await
            .unwrap_or_else(|panic_info| {
                // Convert panic to ExitReason
                let panic_msg = if let Some(s) = panic_info.downcast_ref::<&str>() {
                    s.to_string()
                } else if let Some(s) = panic_info.downcast_ref::<String>() {
                    s.clone()
                } else {
                    "Actor panicked".to_string()
                };

                tracing::error!(
                    "Actor {} ({}) panicked: {}",
                    pid,
                    actor_type_for_panic,
                    panic_msg
                );
                ActorMetrics::actor_panicked_typed(&actor_type_for_panic);
                ExitReason::Panic(panic_msg)
            });

            // Cleanup - always runs even if actor panicked
            system.cleanup_actor(pid, &exit_reason).await;

            exit_reason
        });

        ActorRef {
            pid,
            system: Arc::clone(self),
            join_handle: Some(join_handle),
        }
    }

    /// Sends a message to an actor (local or remote).
    ///
    /// This method provides location transparency - it automatically routes
    /// messages to local or remote actors based on the Pid's node ID.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use joerl::{ActorSystem, Pid};
    /// # async fn example(system: std::sync::Arc<ActorSystem>, pid: Pid) {
    /// // Works for both local and remote Pids!
    /// system.send(pid, Box::new("Hello")).await.unwrap();
    /// # }
    /// ```
    pub async fn send(&self, to: Pid, msg: Message) -> Result<()> {
        // Normalize Pid for location transparency
        let normalized_to = self.normalize_pid(to);

        // Determine if this is a local or remote message
        let is_local = normalized_to.node() == 0;

        if is_local {
            // Local message - deliver directly
            let result = if let Some(entry) = self.actors.get(&normalized_to) {
                entry
                    .sender
                    .send(Envelope::message(msg))
                    .await
                    .map_err(|_| ActorError::SendFailed(to))
            } else {
                Err(ActorError::ActorNotFound(to))
            };

            match &result {
                Ok(_) => MessageMetrics::message_sent(),
                Err(ActorError::SendFailed(_)) => {
                    MessageMetrics::message_send_failed("mailbox_full")
                }
                Err(ActorError::ActorNotFound(_)) => {
                    MessageMetrics::message_send_failed("actor_not_found")
                }
                _ => {}
            }

            result
        } else {
            // Remote message - forward via network
            if let Some(node_registry) = &self.node_registry {
                crate::distributed::send_remote(
                    node_registry,
                    Pid::with_node(self.local_node_id, 0), // Placeholder sender
                    to,
                    msg,
                )
                .await
                .map_err(|e| ActorError::other(format!("Remote send failed: {}", e)))?;
                MessageMetrics::message_sent();
                Ok(())
            } else {
                // Not a distributed system, can't send remote messages
                Err(ActorError::other(format!(
                    "Cannot send to remote Pid {} - system is not distributed",
                    to
                )))
            }
        }
    }

    /// Attempts to send a message without blocking.
    pub fn try_send(&self, to: Pid, msg: Message) -> Result<()> {
        let result = if let Some(entry) = self.actors.get(&to) {
            entry
                .sender
                .try_send(Envelope::message(msg))
                .map_err(|_| ActorError::SendFailed(to))
        } else {
            Err(ActorError::ActorNotFound(to))
        };

        match &result {
            Ok(_) => MessageMetrics::message_sent(),
            Err(ActorError::SendFailed(_)) => MessageMetrics::message_send_failed("mailbox_full"),
            Err(ActorError::ActorNotFound(_)) => {
                MessageMetrics::message_send_failed("actor_not_found")
            }
            _ => {}
        }

        result
    }

    /// Sends a signal to an actor.
    pub(crate) async fn send_signal(&self, to: Pid, signal: Signal) -> Result<()> {
        // Telemetry: track signal sending
        let signal_type = match &signal {
            Signal::Exit { .. } => "exit",
            Signal::Down { .. } => "down",
            Signal::Stop => "stop",
            Signal::Kill => "kill",
        };

        // Track exit reasons specifically
        if let Signal::Exit { reason, .. } = &signal {
            let reason_str = match reason {
                ExitReason::Normal => "normal",
                ExitReason::Shutdown => "shutdown",
                ExitReason::Killed => "killed",
                ExitReason::Panic(_) => "panic",
                ExitReason::Custom(_) => "custom",
            };
            SignalMetrics::exit_signal_by_reason(reason_str);
        }

        SignalMetrics::signal_sent(signal_type);

        if let Some(entry) = self.actors.get(&to) {
            entry
                .sender
                .send(Envelope::signal(signal))
                .await
                .map_err(|_| ActorError::SendFailed(to))
        } else {
            Err(ActorError::ActorNotFound(to))
        }
    }

    /// Links two actors bidirectionally.
    ///
    /// Creates a bidirectional link between two actors. When either actor
    /// terminates abnormally, the other receives an EXIT signal and will
    /// terminate unless it's trapping exits.
    ///
    /// In Erlang: `link(Pid)`
    ///
    /// # Arguments
    ///
    /// * `pid1` - The first actor's Pid
    /// * `pid2` - The second actor's Pid
    ///
    /// # Errors
    ///
    /// Returns `ActorError::ActorNotFound` if either actor doesn't exist.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use joerl::{ActorSystem, Actor, ActorContext, Message};
    /// use async_trait::async_trait;
    ///
    /// struct Worker;
    ///
    /// #[async_trait]
    /// impl Actor for Worker {
    ///     async fn handle_message(&mut self, _msg: Message, _ctx: &mut ActorContext) {}
    /// }
    ///
    /// # tokio_test::block_on(async {
    /// let system = ActorSystem::new();
    /// let worker1 = system.spawn(Worker);
    /// let worker2 = system.spawn(Worker);
    ///
    /// system.link(worker1.pid(), worker2.pid()).expect("Failed to link");
    /// # });
    /// ```
    pub fn link(&self, pid1: Pid, pid2: Pid) -> Result<()> {
        // Add link in both directions
        if let Some(mut entry) = self.actors.get_mut(&pid1) {
            entry.links.insert(pid2);
        } else {
            return Err(ActorError::ActorNotFound(pid1));
        }

        if let Some(mut entry) = self.actors.get_mut(&pid2) {
            entry.links.insert(pid1);
        } else {
            return Err(ActorError::ActorNotFound(pid2));
        }

        LinkMetrics::link_created();
        Ok(())
    }

    /// Monitors an actor (unidirectional).
    ///
    /// Creates a unidirectional monitor from one actor to another. When the
    /// monitored actor terminates, the monitoring actor receives a DOWN signal
    /// with the monitor reference, but is not affected by the termination.
    ///
    /// In Erlang: `monitor(process, Pid)`
    ///
    /// # Arguments
    ///
    /// * `from` - The Pid of the monitoring actor
    /// * `to` - The Pid of the actor to monitor
    ///
    /// # Returns
    ///
    /// A `MonitorRef` that will be included in the DOWN signal
    ///
    /// # Errors
    ///
    /// Returns `ActorError::ActorNotFound` if either actor doesn't exist.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use joerl::{ActorSystem, Actor, ActorContext, Message};
    /// use async_trait::async_trait;
    ///
    /// struct Worker;
    ///
    /// #[async_trait]
    /// impl Actor for Worker {
    ///     async fn handle_message(&mut self, _msg: Message, _ctx: &mut ActorContext) {}
    /// }
    ///
    /// # tokio_test::block_on(async {
    /// let system = ActorSystem::new();
    /// let observer = system.spawn(Worker);
    /// let worker = system.spawn(Worker);
    ///
    /// let monitor_ref = system.monitor(observer.pid(), worker.pid())
    ///     .expect("Failed to monitor");
    /// # });
    /// ```
    pub fn monitor(&self, from: Pid, to: Pid) -> Result<MonitorRef> {
        let monitor_ref = MonitorRef::new(MONITOR_COUNTER.fetch_add(1, Ordering::Relaxed));

        let result = if let Some(mut entry) = self.actors.get_mut(&to) {
            entry.monitors.insert((from, monitor_ref));
            Ok(monitor_ref)
        } else {
            Err(ActorError::ActorNotFound(to))
        };

        if result.is_ok() {
            LinkMetrics::monitor_created();
        }

        result
    }

    /// Returns true if an actor with the given Pid exists and is running.
    ///
    /// This checks if an actor is still registered in the system. Returns false
    /// if the actor never existed or has already terminated.
    ///
    /// Note: For checking if actors are alive (including remote ones), use
    /// `is_process_alive()` which supports both local and remote Pids.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use joerl::{ActorSystem, Actor, ActorContext, Message, Pid};
    /// use async_trait::async_trait;
    ///
    /// struct Worker;
    ///
    /// #[async_trait]
    /// impl Actor for Worker {
    ///     async fn handle_message(&mut self, _msg: Message, _ctx: &mut ActorContext) {}
    /// }
    ///
    /// # tokio_test::block_on(async {
    /// let system = ActorSystem::new();
    /// let worker = system.spawn(Worker);
    ///
    /// assert!(system.is_actor_alive(worker.pid()));
    ///
    /// // Non-existent actor
    /// let fake_pid = Pid::new();
    /// assert!(!system.is_actor_alive(fake_pid));
    /// # });
    /// ```
    pub fn is_actor_alive(&self, pid: Pid) -> bool {
        self.actors.contains_key(&pid)
    }

    /// Returns the number of currently active actors in the system.
    ///
    /// This is useful for monitoring and health checks.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use joerl::{ActorSystem, Actor, ActorContext, Message};
    /// use async_trait::async_trait;
    ///
    /// struct Worker;
    ///
    /// #[async_trait]
    /// impl Actor for Worker {
    ///     async fn handle_message(&mut self, _msg: Message, _ctx: &mut ActorContext) {}
    /// }
    ///
    /// # tokio_test::block_on(async {
    /// let system = ActorSystem::new();
    /// assert_eq!(system.actor_count(), 0);
    ///
    /// let _worker1 = system.spawn(Worker);
    /// let _worker2 = system.spawn(Worker);
    /// assert_eq!(system.actor_count(), 2);
    /// # });
    /// ```
    pub fn actor_count(&self) -> usize {
        self.actors.len()
    }

    // ========================================================================
    // Erlang-style Helper Functions
    // ========================================================================

    /// Returns true if this is a distributed (alive) node.
    ///
    /// Equivalent to Erlang's `erlang:is_alive/0`.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use joerl::ActorSystem;
    ///
    /// let local_system = ActorSystem::new();
    /// assert!(!local_system.is_distributed());
    ///
    /// // Distributed system would return true
    /// // let dist_system = ActorSystem::new_distributed(...).await.unwrap();
    /// // assert!(dist_system.is_distributed());
    /// ```
    pub fn is_distributed(&self) -> bool {
        self.local_node_id != 0
    }

    /// Returns the list of connected nodes.
    ///
    /// For local-only systems, returns an empty vector.
    /// Equivalent to Erlang's `erlang:nodes/0`.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use joerl::ActorSystem;
    ///
    /// let system = ActorSystem::new();
    /// assert_eq!(system.nodes(), Vec::<String>::new());
    /// ```
    pub fn nodes(&self) -> Vec<String> {
        if let Some(registry) = &self.node_registry {
            registry.list_connected_nodes()
        } else {
            Vec::new()
        }
    }

    /// Returns this node's name (if distributed).
    ///
    /// Returns `None` for local-only systems.
    /// Equivalent to Erlang's `erlang:node/0`.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use joerl::ActorSystem;
    ///
    /// let system = ActorSystem::new();
    /// assert_eq!(system.node(), None);
    /// ```
    pub fn node(&self) -> Option<&str> {
        self.node_name.as_deref()
    }

    /// Returns the node name for a given Pid.
    ///
    /// Returns `None` if:
    /// - The Pid is local (node==0) and system is not distributed
    /// - The node ID cannot be resolved to a name
    ///
    /// Equivalent to Erlang's `erlang:node/1`.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use joerl::{ActorSystem, Pid};
    ///
    /// let system = ActorSystem::new();
    /// let local_pid = Pid::new();
    /// assert_eq!(system.node_of(local_pid), None);
    /// ```
    pub fn node_of(&self, pid: Pid) -> Option<String> {
        let node_id = pid.node();

        if node_id == 0 {
            // Local pid - return our node name if distributed
            return self.node_name.clone();
        }

        if node_id == self.local_node_id {
            // It's actually our local node
            return self.node_name.clone();
        }

        // Remote pid - lookup in registry
        if let Some(registry) = &self.node_registry {
            registry.get_node_name(node_id)
        } else {
            None
        }
    }

    /// Checks if a process (actor) is alive.
    ///
    /// For local Pids, checks the local registry.
    /// For remote Pids, sends a ping/pong RPC request with a timeout.
    ///
    /// Equivalent to Erlang's `erlang:is_process_alive/1`.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use joerl::{ActorSystem, Actor, ActorContext, Message};
    /// use async_trait::async_trait;
    ///
    /// struct Worker;
    ///
    /// #[async_trait]
    /// impl Actor for Worker {
    ///     async fn handle_message(&mut self, _msg: Message, _ctx: &mut ActorContext) {}
    /// }
    ///
    /// # tokio_test::block_on(async {
    /// let system = ActorSystem::new();
    /// let worker = system.spawn(Worker);
    ///
    /// assert!(system.is_process_alive(worker.pid()).await);
    /// # });
    /// ```
    pub async fn is_process_alive(&self, pid: Pid) -> bool {
        // Normalize the Pid first
        let normalized = self.normalize_pid(pid);

        if normalized.node() == 0 {
            // Local check - just look in registry
            self.actors.contains_key(&normalized)
        } else {
            // Remote check - use ping/pong RPC
            if let Some(registry) = &self.node_registry {
                crate::distributed::ping_process(registry, pid)
                    .await
                    .unwrap_or(false)
            } else {
                false
            }
        }
    }

    /// Connects to a remote node by name.
    ///
    /// Discovers the node via EPMD and establishes a connection.
    /// The remote node will automatically connect back (bidirectional).
    ///
    /// Returns an error if:
    /// - System is not distributed
    /// - Node not found in EPMD
    /// - Connection failed
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use joerl::ActorSystem;
    ///
    /// # async fn example() {
    /// let system = ActorSystem::new_distributed(
    ///     "node_a",
    ///     "127.0.0.1:5000",
    ///     "127.0.0.1:4369"
    /// ).await.unwrap();
    ///
    /// system.connect_to_node("node_b").await.unwrap();
    /// # }
    /// ```
    pub async fn connect_to_node(&self, node_name: &str) -> Result<()> {
        if let (Some(epmd), Some(registry), Some(my_name), Some(my_addr)) = (
            &self.epmd_client,
            &self.node_registry,
            &self.node_name,
            &self.listen_address,
        ) {
            crate::distributed::connect_to_node(
                epmd,
                registry,
                node_name,
                my_name,
                self.local_node_id,
                my_addr,
            )
            .await
            .map_err(|e| ActorError::other(format!("Connection failed: {}", e)))
        } else {
            Err(ActorError::other(
                "Cannot connect - system is not distributed".to_string(),
            ))
        }
    }

    /// Cleans up an actor after termination.
    async fn cleanup_actor(&self, pid: Pid, reason: &ExitReason) {
        // Get actor type, spawn time, and remove from registry
        let (actor_type, links, monitors, spawn_time) =
            if let Some((_, entry)) = self.actors.remove(&pid) {
                (
                    entry.actor_type,
                    entry.links,
                    entry.monitors,
                    #[cfg(feature = "telemetry")]
                    entry.spawn_time,
                    #[cfg(not(feature = "telemetry"))]
                    std::time::Instant::now(),
                )
            } else {
                return;
            };

        // Telemetry: record actor stop with type
        let reason_str = match reason {
            ExitReason::Normal => "normal",
            ExitReason::Shutdown => "shutdown",
            ExitReason::Killed => "killed",
            ExitReason::Panic(_) => "panic",
            ExitReason::Custom(_) => "custom",
        };
        ActorMetrics::actor_stopped_typed(&actor_type, reason_str);

        // Telemetry: record actor lifetime
        #[cfg(feature = "telemetry")]
        {
            let lifetime = spawn_time.elapsed().as_secs_f64();
            ActorMetrics::actor_lifetime(&actor_type, lifetime);
        }

        // Send EXIT signals to linked actors
        for linked_pid in links {
            let _ = self
                .send_signal(
                    linked_pid,
                    Signal::Exit {
                        from: pid,
                        reason: reason.clone(),
                    },
                )
                .await;
        }

        // Send DOWN signals to monitoring actors
        for (monitoring_pid, monitor_ref) in monitors {
            let _ = self
                .send_signal(
                    monitoring_pid,
                    Signal::Down {
                        reference: monitor_ref,
                        pid,
                        reason: reason.clone(),
                    },
                )
                .await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actor::Actor;
    use async_trait::async_trait;

    struct TestActor;

    #[async_trait]
    impl Actor for TestActor {
        async fn handle_message(&mut self, _msg: Message, _ctx: &mut ActorContext) {}
    }

    #[tokio::test]
    async fn test_system_new() {
        let system = ActorSystem::new();
        assert!(system.actors.is_empty());
    }

    #[tokio::test]
    async fn test_spawn_actor() {
        let system = ActorSystem::new();
        let actor_ref = system.spawn(TestActor);
        assert!(system.is_actor_alive(actor_ref.pid()));
    }

    #[tokio::test]
    async fn test_send_message() {
        let system = ActorSystem::new();
        let actor_ref = system.spawn(TestActor);

        let result = actor_ref.send(Box::new(42i32)).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_send_to_dead_actor() {
        let system = ActorSystem::new();
        let pid = Pid::new();

        let result = system.send(pid, Box::new(42i32)).await;
        assert!(matches!(result, Err(ActorError::ActorNotFound(_))));
    }

    #[tokio::test]
    async fn test_link_actors() {
        let system = ActorSystem::new();
        let actor1 = system.spawn(TestActor);
        let actor2 = system.spawn(TestActor);

        let result = system.link(actor1.pid(), actor2.pid());
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_monitor_actor() {
        let system = ActorSystem::new();
        let actor1 = system.spawn(TestActor);
        let actor2 = system.spawn(TestActor);

        let result = system.monitor(actor1.pid(), actor2.pid());
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_is_actor_alive() {
        let system = ActorSystem::new();
        let actor_ref = system.spawn(TestActor);

        assert!(system.is_actor_alive(actor_ref.pid()));

        let dead_pid = Pid::new();
        assert!(!system.is_actor_alive(dead_pid));
    }

    #[tokio::test]
    async fn test_is_distributed() {
        // Local-only system
        let local_system = ActorSystem::new();
        assert!(!local_system.is_distributed());
        assert_eq!(local_system.local_node_id(), 0);
    }

    #[tokio::test]
    async fn test_nodes_empty_for_local() {
        let system = ActorSystem::new();
        assert_eq!(system.nodes(), Vec::<String>::new());
    }

    #[tokio::test]
    async fn test_node_none_for_local() {
        let system = ActorSystem::new();
        assert_eq!(system.node(), None);
    }

    #[tokio::test]
    async fn test_node_of_local_pid() {
        let system = ActorSystem::new();
        let pid = Pid::new();
        assert_eq!(system.node_of(pid), None);
    }

    #[tokio::test]
    async fn test_is_process_alive_local() {
        let system = ActorSystem::new();
        let actor = system.spawn(TestActor);

        // Local actor should be alive
        assert!(system.is_process_alive(actor.pid()).await);

        // Non-existent local actor
        let fake_pid = Pid::new();
        assert!(!system.is_process_alive(fake_pid).await);
    }

    #[tokio::test]
    async fn test_pid_normalization_local_system() {
        let system = ActorSystem::new();
        let local_pid = Pid::new();

        // In local system, normalize should keep node==0
        let normalized = system.normalize_pid(local_pid);
        assert_eq!(normalized.node(), 0);

        // Even if we pass a pid with different node, it should normalize to 0
        let remote_looking_pid = Pid::with_node(42, local_pid.id());
        let normalized2 = system.normalize_pid(remote_looking_pid);
        assert_eq!(normalized2.node(), 0);
    }

    #[tokio::test]
    async fn test_connect_to_node_requires_distributed() {
        let system = ActorSystem::new();
        let result = system.connect_to_node("some_node").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not distributed"));
    }
}
