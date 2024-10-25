# 证明者的流程设计

* 这个地方需要考虑一下 多个CPU.
* 关键地方的设计
* 关键地方的输入和输出
* 这个地方的代码是怎么 从 OS 到 VM 中的.(这个地方也曾经画了一周的时间.)
* 它又是怎么用 VM 中的返回值的代码的.
* 把 整个输入输出 给串起来，就是非常好的 设计了, 代码不能太大，也不能太小.

* 先用文字描述清楚后，才可以画图等等来表示. 一步一步的流程 走起来即可.

***

## 1.证明者本身的信息:

这个地方 重点关注 puzzle

```rust

/// A prover is a light node, capable of producing proofs for consensus.
#[derive(Clone)]
pub struct Prover<N: Network, C: ConsensusStorage<N>> {
    /// The router of the node.
    router: Router<N>,
    /// The sync module.
    sync: Arc<BlockSync<N>>,
    /// The genesis block.
    genesis: Block<N>,
    /// The puzzle.
    puzzle: Puzzle<N>, // [plh] 跟 puzzle 有关系的 1
    /// The latest epoch hash.
    latest_epoch_hash: Arc<RwLock<Option<N::BlockHash>>>,
    /// The latest block header.
    latest_block_header: Arc<RwLock<Option<Header<N>>>>,
    /// The number of puzzle instances.
    puzzle_instances: Arc<AtomicU8>, // [plh] 跟 puzzle 有关系的2，puzzle_instances 实例数量
    /// The maximum number of puzzle instances.
    max_puzzle_instances: u8, // [plh] 跟puzzle 有关系的3, max_puzzle_instances 允许的 puzzle 数量
    /// The spawned handles.
    handles: Arc<Mutex<Vec<JoinHandle<()>>>>,
    /// The shutdown signal.
    shutdown: Arc<AtomicBool>,
    /// PhantomData.
    _phantom: PhantomData<C>,

}

```

## 2 证明者的初始化

接下来看看 初始化. 同样的关注是 puzzle 这个地方 也必须跟上面一致，也是3个.

```rust
impl<N: Network, C: ConsensusStorage<N>> Prover<N, C> {
    /// Initializes a new prover node.
    pub async fn new(
        node_ip: SocketAddr,
        account: Account<N>,
        trusted_peers: &[SocketAddr],
        genesis: Block<N>,
        storage_mode: StorageMode,
        shutdown: Arc<AtomicBool>,
    ) -> Result<Self> {
        // Initialize the signal handler.
        let signal_node = Self::handle_signals(shutdown.clone());

        // Initialize the ledger service.
        let ledger_service = Arc::new(ProverLedgerService::new());
        // Initialize the sync module.
        let sync = BlockSync::new(BlockSyncMode::Router, ledger_service.clone());
        // Determine if the prover should allow external peers.
        let allow_external_peers = true;

        // Initialize the node router.
        let router = Router::new(
            node_ip,
            NodeType::Prover,
            account,
            trusted_peers,
            Self::MAXIMUM_NUMBER_OF_PEERS as u16,
            allow_external_peers,
            matches!(storage_mode, StorageMode::Development(_)),
        )
            .await?;
        // Compute the maximum number of puzzle instances.
        let max_puzzle_instances = num_cpus::get().saturating_sub(2).clamp(1, 6); // 取值一般都是 6

        // Initialize the node.
        let node = Self {
            router,
            sync: Arc::new(sync),
            genesis,
            puzzle: VM::<N, C>::new_puzzle()?, // [plh] puzzle 初始化了， 因为后面需要调用 puzzle 中的方法 . 这个地方 才是第一次出现 VM的地方. 这个地方需要改的地方. 这个地方才是 puzzle 用武之地.
            latest_epoch_hash: Default::default(),
            latest_block_header: Default::default(),
            puzzle_instances: Default::default(), // puzzle 数量的当前值, 默认为 0 ,一开始肯定是0, 这个地方也是 原子计数，后面可以仿造这个来进行计数.
            max_puzzle_instances: u8::try_from(max_puzzle_instances)?, // puzzle 的最大值
            handles: Default::default(),
            shutdown,
            _phantom: Default::default(),
        };
        // Initialize the routing.
        node.initialize_routing().await;
        // Initialize the puzzle.
        node.initialize_puzzle().await; // [plh] 这个地方的原始注释是写错了，这个地方实际是调用 puzzle 中的方法了, 已经不是初始化了.

        // Initialize the notification message loop.
        node.handles.lock().push(crate::start_notification_message_loop());
        // Pass the node to the signal handler.
        let _ = signal_node.set(node.clone());
        // Return the node.
        Ok(node)
    }
}

```

后面这个的地方 流程比较 细了.

前面已经 puzzle 初始化了， 看看如何执行的.

## 3.0 node.initialize_puzzle().await;

```rust
impl<N: Network, C: ConsensusStorage<N>> Prover<N, C> {
    /// Initialize a new instance of the puzzle.
    async fn initialize_puzzle(&self) {
        // [plh] 后面要根据 显卡的数量来看相应的 tokio::spawn 异步， 这个地方需要注意一下
        for _ in 0..self.max_puzzle_instances {
            // 允许的最大值 max_puzzle_instances  就开几个 tokio::spawn 异步
            let prover = self.clone();
            self.handles.lock().push(tokio::spawn(async move {
                prover.puzzle_loop().await; // 这个地方是 死循环
            }));
        }
    }
}

```

## 3.1 prover.puzzle_loop()

```rust

impl<N: Network, C: ConsensusStorage<N>> Prover<N, C> {
    /// Executes an instance of the puzzle.
    async fn puzzle_loop(&self) {
        // 从名字就知道 这个地方要一直计算才行, 是个死循环的. 因为一直要计算的.
        // 既然是 死循环，那么什么样的条件 跳出，什么样的条件 继续 , 以及还要处理下 中断信号才行.

        loop {
            // 是否链接的有 其他路由节点，如果没有链接，那么不需要。休息一下 .  continue
            // 为什么需要这个逻辑， 其原因应该是 计算出结果后需要广播出去。 广播的前提也是一样的，必须有节点链接， 这个地方相当于打了 提前量.

            // [plh] 因为后面节点单独出来，这个地方的逻辑会取消, 也就是 无条件继续执行.
            // If the node is not connected to any peers, then skip this iteration.
            if self.router.number_of_connected_peers() == 0 {
                debug!("Skipping an iteration of the puzzle (no connected peers)");
                tokio::time::sleep(Duration::from_secs(N::ANCHOR_TIME as u64)).await;
                continue;
            }

            // If the number of instances of the puzzle exceeds the maximum, then skip this iteration.
            // 添加 max_puzzle_instances 最大限制.
            if self.num_puzzle_instances() > self.max_puzzle_instances {
                // Sleep for a brief period of time.
                tokio::time::sleep(Duration::from_millis(500)).await;
                continue;
            }

            // 从链上 取数据了
            // 最新的 latest_epoch_hash
            // 获取到 需要证明的 `coinbase_target` `proof_target` 这些都是 块上的数据结构
            // 块上的数据 不一定有这个， 所以用的 Some

            // Read the latest epoch hash.
            let latest_epoch_hash = *self.latest_epoch_hash.read();
            // Read the latest state.
            let latest_state = self
                .latest_block_header
                .read()
                .as_ref()
                .map(|header| (header.coinbase_target(), header.proof_target()));

            // If the latest epoch hash and latest state exists, then proceed to generate a solution.
            if let (Some(epoch_hash), Some((coinbase_target, proof_target))) = (latest_epoch_hash, latest_state) {
                // Execute the puzzle.
                let prover = self.clone();
                let result = tokio::task::spawn_blocking(move || {
                    // 如何使用的这个结果的，封装一下 puzzle_loop 和 puzzle_iteration 刚好对应一下, 返回值是什么?
                    // solution_target:u64 solution:Solution
                    // 这两个值之间是什么关系 ？
                    //
                    // 但是后面广播的时候 只需要  solution 广播出去. (solution 包含了 solution_target 有.)

                    prover.puzzle_iteration(epoch_hash, coinbase_target, proof_target, &mut OsRng)
                    // 传递了 随机值进去了,因为这个地方的 随机值也是构成 solution 的一部分，否则无法验证者无法校验
                })
                    .await;

                // If the prover found a solution, then broadcast it.
                if let Ok(Some((solution_target, solution))) = result {
                    info!("Found a Solution '{}' (Proof Target {solution_target})", solution.id());
                    // Broadcast the solution.
                    self.broadcast_solution(solution); // 广播是把这个整体给广播出去，不然的话验证者如何验证这个那.
                }
            } else {
                // Otherwise, sleep for a brief period of time, to await for puzzle state.
                tokio::time::sleep(Duration::from_secs(1)).await;
            }

            // If the Ctrl-C handler registered the signal, stop the prover.
            if self.shutdown.load(Ordering::Relaxed) {
                debug!("Shutting down the puzzle...");
                break;
            }
        }
    }
}

```

## 3.2 prover.puzzle_iteration 这个地方是为 puzzle_loop 做的对应封装，否则代码量太大了.

```rust

impl<N: Network, C: ConsensusStorage<N>> Prover<N, C> {
    /// Performs one iteration of the puzzle.
    fn puzzle_iteration<R: Rng + CryptoRng>(
        &self,
        epoch_hash: N::BlockHash, // 块上数据
        coinbase_target: u64,     // 块上数据
        proof_target: u64,        // 块上数据
        rng: &mut R,              // 随机数
    ) -> Option<(u64, Solution<N>)> {
        // Increment the puzzle instances.
        // 证明之前 增加 puzzle 数量
        self.increment_puzzle_instances();

        debug!(
                "Proving 'Puzzle' for Epoch '{}' {}",
                fmt_id(epoch_hash),
                format!("(Coinbase Target {coinbase_target}, Proof Target {proof_target})").dimmed()
            );

        // Compute the solution.
        // 这个地方 其实也是 之前 OS 到 VM 中的 puzzle.prove
        // 注意它这个地方的使用方式
        let result =
            self.puzzle.prove(epoch_hash, self.address(), rng.gen(), Some(proof_target)).ok().and_then(|solution| {
                self.puzzle.get_proof_target(&solution).ok().map(|solution_target| (solution_target, solution))
            });

        // Decrement the puzzle instances.
        // 证明之后 减少数量
        self.decrement_puzzle_instances();
        // Return the result.
        result
    }
}

```

## 3.3  broadcast_solution

广播的 就是 Solution 可以这么说.

```rust


impl<N: Network, C: ConsensusStorage<N>> Prover<N, C> {
    /// Broadcasts the solution to the network.

    fn broadcast_solution(&self, solution: Solution<N>) {
        // Prepare the unconfirmed solution message.
        let message = Message::UnconfirmedSolution(UnconfirmedSolution {
            solution_id: solution.id(),
            solution: Data::Object(solution),
        });
        // Propagate the "UnconfirmedSolution".
        self.propagate(message, &[]);
    }
}

```






















