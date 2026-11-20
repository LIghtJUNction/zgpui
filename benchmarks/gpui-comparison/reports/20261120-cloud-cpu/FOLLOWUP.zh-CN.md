# CPU 对照之后：生产路径与下一项可执行改进

本轮数据说明 zgpui 在固定场景的新建和调整尺寸任务上用时较少，也暴露了较大保留布局和部分几何查询的差距。以下只分析冻结源码并提出下一步；**没有修改生产算法、重新采样或证明应用帧率提升**。原始样本、统计表和计数限制保持在 [4b6d628 报告快照](https://github.com/LIghtJUNction/zgpui/tree/4b6d628452f62dcbdb955b98183dfe134d173ad1/benchmarks/gpui-comparison/reports/20261120-cloud-cpu)。

## 1. 要解释的差距

表中比例的计算方式是：每轮分别求 GPUI 与 zgpui 的逐操作时间均值，再计算两者之比，最后取七轮比值的几何平均。95% 区间按配对轮次 bootstrap；不是 p50 比值，也不是全框架评分。[统计实现](https://github.com/LIghtJUNction/zgpui/blob/4b6d628452f62dcbdb955b98183dfe134d173ad1/benchmarks/gpui-comparison/report.py#L17-L22)

| 总节点数 | 任务 | GPUI / zgpui 比例及 95% 区间 | zgpui / GPUI 的 p50 |
| ---: | --- | ---: | ---: |
| 73 | 保留布局 | 0.788 [0.624, 0.944] | 3,743 / 3,285 ns |
| 241 | 保留布局 | 0.805 [0.760, 0.854] | 13,088 / 10,866 ns |
| 21 | 几何查询 | 0.784 [0.578, 0.994] | 47 / 45 ns |
| 241 | 几何查询 | 0.889 [0.814, 0.973] | 238 / 216 ns |

3、73 节点几何查询的区间跨过 1，本轮不能据此确定差异方向。所有数字来自既有 [summary.json](https://github.com/LIghtJUNction/zgpui/blob/4b6d628452f62dcbdb955b98183dfe134d173ad1/benchmarks/gpui-comparison/reports/20261120-cloud-cpu/summary.json)。源码分析可以说明两条路径做了哪些工作，但没有逐函数性能剖析，不能把总差距按百分比分摊给某一函数。

## 2. 保留布局仍在执行哪些工作

**zgpui 的保留任务每次创建一个新的布局快照。** 夹具保留 `Scene`，但仍调用生产 `Layout::new`，提取全部叶子矩形并计算校验和。`Layout::new` 每次先执行 `validate_scene`，再申请 `Vec<Item>` 并递归放置节点。图校验、自然尺寸测量和放置过程分别通过二分查找把节点 ID 转为数组下标；`Column` 的放置还会调用 `measure`。这里存在重复解析和遍历，但不能把这个有深度上限的树统称为平方复杂度。[夹具](https://github.com/LIghtJUNction/zgpui/blob/4b6d628452f62dcbdb955b98183dfe134d173ad1/benchmarks/gpui-comparison/src/main.rs#L390-L413)、[布局](https://github.com/LIghtJUNction/zgpui/blob/1d796d1fc63808dc099ed7b10c92e75835ae2814/src/layout.rs#L77-L207)、[校验](https://github.com/LIghtJUNction/zgpui/blob/1d796d1fc63808dc099ed7b10c92e75835ae2814/src/lib.rs#L355-L459)。

**GPUI 保留的是求解器缓存，不是直接返回全部旧矩形。** 相同布局输入可以命中 Taffy 0.15.0 的 `compute_cached_layout`。与此同时，GPUI 的 `compute_layout` 在重复调用时仍遍历节点，清除 `absolute_layout_bounds` 和 `absolute_outer_origins`；夹具随后逐叶调用 `layout_bounds`，重新得到绝对矩形。比较保留了这套正常缓存行为，没有人为清空 Taffy 求解缓存。[GPUI 重算入口](https://github.com/zed-industries/zed/blob/a246423a3b24a08e0d0c8574eab3a9de422747e0/crates/gpui/src/taffy.rs#L187-L279)、[绝对坐标](https://github.com/zed-industries/zed/blob/a246423a3b24a08e0d0c8574eab3a9de422747e0/crates/gpui/src/taffy.rs#L356-L389)、[锁定 Taffy 源码的缓存入口](https://github.com/DioxusLabs/taffy/blob/eb6450694f33e2b7413e40677f230ff63f0fc194/src/compute/mod.rs#L326-L361)。

独立分配探针为这一区别提供了补充证据：241 节点保留布局中，zgpui 每次申请一次、5,784 B；GPUI 每次申请 16 次、1,920 B。前者与 `Vec<Item>` 按全部节点预留空间一致；后者与 GPUI 遍历中调用 Taffy `children()` 克隆非空子节点数组的路径一致。这里只说明源码与观测相符，未将全部耗时归因于分配，也未测出库独占的活跃堆或 RSS。[分配表](https://github.com/LIghtJUNction/zgpui/blob/4b6d628452f62dcbdb955b98183dfe134d173ad1/benchmarks/gpui-comparison/reports/20261120-cloud-cpu/report.md)、[children 实现](https://github.com/DioxusLabs/taffy/blob/eb6450694f33e2b7413e40677f230ff63f0fc194/src/tree/taffy_tree.rs#L881-L886)。

## 3. 几何查询不是完整输入分发

两条查询路径执行相同坐标序列并通过独立的半开矩形预期，但接口义务不同。zgpui 的生产 `button_at` 检查场景身份、最小按钮高度、矩形、节点类型和启用状态；GPUI 侧是夹具提供的 `Bounds` 线性宿主适配器，不是完整的 GPUI 事件系统。不能删掉 zgpui 的身份或启用检查来取得更好的数字。[zgpui 查询](https://github.com/LIghtJUNction/zgpui/blob/1d796d1fc63808dc099ed7b10c92e75835ae2814/src/layout.rs#L209-L224)、[两侧计时路径](https://github.com/LIghtJUNction/zgpui/blob/4b6d628452f62dcbdb955b98183dfe134d173ad1/benchmarks/gpui-comparison/src/main.rs#L390-L453)。

源码中，zgpui 在几何拒绝之前调用 `scene.nodes.get(item.index)`。可做的最小独立试验是先拒绝高度不足或矩形未命中的 item，再查询节点。应保留检查语义、命中顺序、溢出处理和禁用按钮行为。编译器可能已调整部分加载顺序，**这只是待验证的改进候选，不是已经确认的提速原因或收益**。

## 4. 当前 ztty 实际调用了什么

核对范围为本轮纳管的 42 个仓库、60 个 Cargo 包，不扩大为全部外部使用者。实际 `impact --repositories zgpui` 返回 `zgpui`、`zterm`、`ztty`、`oh-my-ztty`；其中前两条消费边属于 Rust 包关系，套件边属于打包和运行关系。

| 固定消费者 | 实际路径 | 对本轮结论的约束 |
| --- | --- | --- |
| zterm `a2074bac4c030cfd7c0b2514c852a89dcf54648f` | `Cargo.toml` 依赖 zgpui；`src/lib.rs` 重导出为 `ui`，终端状态持有 Scene/Frame | 自身不调用 `layout` 或 `button_at`，不能把微基准当作终端解析性能 |
| ztty `c4f8f48c9598284a589e609f25242b49ceabfea8` | `src/native_ui.rs::Layout::new` 先 `natural_height`，再 `SceneLayout::new` | 真正失效重排时会分别校验、测量；这是应用相关的改进入口 |
| 同一 ztty 源码 | `View::refresh` 比较场景、生命周期 stamp、几何和交互状态；不变时直接返回 | 应用并非每次刷新都执行基准中的保留布局任务 |
| 同一 ztty 源码 | 自有 `Layout::button` 先检查 stamp 和几何，再查节点 | 不调用 zgpui 的 `button_at`；单独优化后者不能直接宣称 ztty 输入更快 |
| oh-my-ztty | 消费正式 ztty 制品及对应安装清单 | 需要实际更新制品和校验和；只升级 Rust manifest 不完成套件传播 |

精确源码：[zterm 依赖](https://github.com/LIghtJUNction/zterm/blob/a2074bac4c030cfd7c0b2514c852a89dcf54648f/Cargo.toml#L16)、[重导出](https://github.com/LIghtJUNction/zterm/blob/a2074bac4c030cfd7c0b2514c852a89dcf54648f/src/lib.rs#L8)、[ztty 自然高度与布局](https://github.com/LIghtJUNction/ztty/blob/c4f8f48c9598284a589e609f25242b49ceabfea8/src/native_ui.rs#L56-L113)、[自有命中路径](https://github.com/LIghtJUNction/ztty/blob/c4f8f48c9598284a589e609f25242b49ceabfea8/src/native_ui.rs#L115-L127)、[刷新缓存](https://github.com/LIghtJUNction/ztty/blob/c4f8f48c9598284a589e609f25242b49ceabfea8/src/native_ui.rs#L146-L174)。

## 5. 下一次实施与验收顺序

**优先研究一次调用内的重复解析，保持原有工作负载。** 在独立生产分支内，让已有图校验的 DFS 同时生成有界的已解析节点索引与子树边界，供 `measure`、`place` 共用；必要时按需记住本次调用的自然高度。候选实现应使用现有 `MAX_NODES`、`MAX_DEPTH` 上限和安全 Rust，不新增堆分配、平台接口或第三方依赖。不要为了建立全量高度表而无条件测量所有被裁掉的子树；记录小场景和裁剪场景可能增加的固定成本。

这一步仍须在每次 `Layout::new` 内完成完整校验；保留无效输入的错误、重复节点/环/深度/文本限制及绘制顺序。不得仅凭公共 `surface/revision` 信任可变 Scene，不得把校验移到计时区间外，不得将保留任务改成直接返回旧答案。查询谓词顺序的试验另作独立提交，便于判断其收益和影响范围。

接着使用两份独立程序重复原有正确性门禁：20 组配置、相同 resize 序列和半开矩形预期、相同命中坐标；补充只针对新实现风险的非连续 ID、深层树、裁剪、饱和算术和无效图回归。正确性通过后，再用原有四种任务、节点数、编译器、锁文件、优化参数、计时与分配分离方式、7 × 5,000 配对方案验证；原始基线不得覆盖。分析 fresh/resize 是否回退，不以某一项领先替代完整结果。

如果以后新增供宿主复用的已校验 Scene API，可考虑合并 ztty 的 `natural_height` 与布局准备，但这是另一项 API 与应用集成工作，必须单独定义生命周期、场景归属、几何及 Metrics 失效规则，并单独记录应用验收，不能悄悄替换本轮基准。

**只有生产补丁实际验收后才发布并传播。** 顺序为 zgpui → zterm → ztty → oh-my-ztty：发布库补丁；更新 zterm 的实际依赖/锁与必要发行；更新 ztty 的 vendored Gitlink、manifest、锁及真实视图/输入回归并产出正式制品；套件接入这些制品和校验和；最后更新聚合图并验证受影响来源。现有 caret 约束允许新版本，不等于消费者已经完成升级。当前文档没有产生新的生产版本，也没有可传播的性能补丁。

本轮仍不涉及 GPU 帧、文本塑形、完整事件分发或 Windows/macOS 性能。69 个线程 CPU 计数零增量和 RSS 收集限制见原报告；不要把它们改写成零 CPU、两库占用内存相同或完整框架全面领先。
