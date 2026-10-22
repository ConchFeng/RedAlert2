# VERA20k Switch 移植

这是原生 Horizon / 大气层移植入口，独立 Cargo workspace，不改变桌面客户端的依赖和锁文件。
当前分支：`feature/switch-port`，上游基线：`38363bba0`。

交接与继续工作见 [HANDOFF.md](HANDOFF.md)。用户最新要求：后续暂时只在模拟器运行验证。

## 已实现并验证

`dist/vera20k-core.nro` 是运行 **原项目 vera20k-core** 的移植探针，
不是之前独立的 Rust hello-world，也还不是可玩的图形客户端。

Eden 0.2.1 / Vulkan / NVIDIA GTX 1650 已验证：

- Rust `std`、文件系统、线程、TLS、系统随机数。
- 生产 MIX 文件加载与文件名查询（Horizon 使用堆内存后备映射）。
- 生产 INI 解析与 CRC。
- Horizon 本地文件时间显示；newlib 不提供 `newlocale/strftime_l`，使用本地时间的数字日期。
- 三线程 Rayon 池及归约。
- 生产 `Simulation` 构造、状态哈希和 16 次 `SimRuntime` 空场景帧事务。
- 原生 libnx 默认窗口、EGL / GLES2 上下文和 1280×720 渐变三角形。
- 手柄 A 键输入及 `+` 正常退出；独立输入层覆盖面键、肩键、菜单键和十字键。
- 检查零售资源根 `sdmc:/game` 中两套主 MIX；存在时通过生产 `AssetManager`
  挂载档案并查找 `rulesmd.ini`。当前测试环境没有资源，因此只验证了缺失分支。

主机与 Switch ARM64 的状态哈希一致：

| 阶段 | 两端哈希 |
| --- | --- |
| 构造，seed=12345 | `cf10ddbe65985acf` |
| 16 帧后 | `3789926a66da78b4` |

这是空场景与指定路径的兼容性证据，不代表零售地图、完整战斗或全部跨平台确定性已经验证。
2026-10-22 已通过 FTP 取回真机核心 NSP 报告并执行 `verify.py --format nsp --report ...`：
上述核心检查、A 键输入和正常退出全部通过，两个状态哈希与主机一致。
独立 Rust 基础测试 NRO 也已通过真机验证；核心 NRO 与基础测试 NSP 尚未取得真机报告。
这是历史记录；用户随后要求暂停全部真机操作。当前和新构建只在 Eden 验证。

## 构建与运行

已有的 SDK 安装沿用 `projects/switch-rust-smoke` 配置，默认位于工作区
`toolchains/switch/sdk/opt/devkitpro`。本入口额外需要：

```bash
rustup toolchain install nightly-2026-10-04 --profile minimal --component rust-src,rustfmt,llvm-tools-preview
```

上游适配代码位于工作区 `toolchains/switch/bevy-horizon-reference`：

```bash
git clone https://github.com/JohnUzoka/bevy-horizon.git $HOME/ws/toolchains/switch/bevy-horizon-reference
git -C $HOME/ws/toolchains/switch/bevy-horizon-reference checkout 3eac779b232ee47cd0eb957676c64386ac7b0f1d
```

本机以上环境已安装。构建入口如下；当前参考仓库包含本地 NVK 探针提交，
再次构建前需按 [HANDOFF.md](HANDOFF.md#下一阶段与已知缺口) 处理固定版本检查：

```bash
cd $HOME/ws/projects/vera20k-switch/platform/switch
python3 build.py
bash run_eden.sh
```

键盘 A 对应 Switch A，键盘 M 对应 `+`。退出后报告在：
`target/emulator/xdg-data/eden/sdmc/switch/vera20k/core-probe-nro.txt`。
截图在 `target/emulator/evidence/input-layer.png`。

常规核心验证使用已安装的 Eden 0.2.1 发布版，不需要构建模拟器。
另行构建 Eden master 仅用于排查 NVK 图形兼容性，不是 Vera 构建的前置步骤。

启动脚本使用独立的 Eden 配置。首次运行若该配置没有 `prod.keys`，
会依次从 `SWITCH_KEYS`、`~/.switch/prod.keys`、默认 Eden 的
`~/.local/share/eden/keys/prod.keys` 导入本机已有密钥，副本权限为 0600。
密钥副本保存在被 Git 忽略的 `target/emulator/xdg-data/eden/keys/`。
若这些位置均无密钥，脚本会明确报错。应用菜单中的 **Eden (Switch Rust Smoke)**
属于另一个工程，其 NRO 配置现已补齐密钥；该工程的 NSP 仍使用独立测试密钥。

`build.py --host-test` 在主机执行同一个生产核心探针。
`build.py --check` 仅执行交叉编译检查。
`build.py --check-libc-abi` 直接复用固定上游版本的 ABI 检查器，在本机比较 SDK C 头与 Rust 类型。
需要 Python 的 `pyelftools`；构建后检查最终 ELF 的 TLS 对齐。
运行 `python3 verify.py` 可核对已保存的模拟器报告、按键/退出状态与主机状态哈希。

## 正式 NSP 与 FTP 真机测试

设备密钥已保存在本机 `~/.switch/prod.keys`，权限为 0600。密钥不放入工程或分发包。

```bash
python3 build.py --nsp
```

生成 `dist/vera20k-core.nro` 和 `dist/vera20k-core.nsp`。
NSP 的应用 ID 为 `0100f52556455241`，直接包含核心程序，不依赖 SD 卡上的 NRO。
打包器用独立 hactool 解密并校验 Program/Control/Meta NCA 的内容哈希与 NPDM，
仅容许自制包预期的固定密钥/ACID 未签名项。核心 NSP 已安装并通过真机运行验证。

```bash
python3 deploy_ftp.py --host <switch-host> --port 5000
```

上传核心和基础 Rust 测试的 NRO/NSP；NRO 放在各自的 `/switch/` 子目录，NSP 放在 `/nsp/`。
上传先写临时文件，再下载逐字节核对后更名；已有不同内容的目标文件会保留备份。
传输清单为 `target/hardware/deployment.json`。

分别从 hbmenu 启动 NRO、从主菜单启动已安装的 NSP，按 A 后按 `+` 退出。
重开 FTP 后下载报告：

```bash
python3 deploy_ftp.py --host <switch-host> --port 5000 --fetch-reports
python3 verify.py --format nro --report target/hardware/reports/core-probe-nro.txt
python3 verify.py --format nsp --report target/hardware/reports/core-probe-nsp.txt
```

NRO 与 NSP 分别写 `sdmc:/switch/vera20k/core-probe-{nro,nsp}.txt`。

## 环境核对与后续移植缺口

| 项目 | 当前情况 |
| --- | --- |
| devkitA64 / libnx / newlib | 已安装，已链接并在模拟器和真机运行 |
| Rust 标准库 / ARM64 libc | 已构建；对 SDK 头文件的 348 项 ABI 对比为 0 不匹配 |
| MIX、INI、模拟核心 / Rayon | 核心 NSP 已通过模拟器和真机验证；范围为合成资源与空场景 |
| Switch SDL2 / EGL / GLES 库 | SDK 中已有，目前客户端尚未接入 |
| Switch 原生窗口 / EGL / GLES2 图形层 | 已在 NRO 和 NSP 的 Eden 运行中画出 1280×720 画面；尚不是 Vera 生产渲染器 |
| wgpu 27 / rodio 0.22 / cpal 0.17 | 使用上游 Horizon 补丁、Vulkan-only 配置时，依赖编译检查通过；尚未链接/运行游戏图形和声音 |
| 桌面 winit / Wayland / rustix | 无法直接用于 Horizon；包含窗口栈的实际依赖检查在 rustix 0.38 报错，winit 本身也没有 Horizon backend |
| Switch NVK 静态驱动 | 已从 Mesa `606441c90e` 交叉编译，生成本地化 `libvulkan_bh.a`；实例、能力枚举、Vi 表面和逻辑设备已通过 |
| NVK 最小探针 | 本地 Eden master `989768a` 已创建交换链和 NAK 图形管线；关闭 macro JIT/HLE 后，30 秒清屏呈现与自动退出通过，但三角形因模拟器未实现 `IMUL (imm)` 着色器指令而未显示 |
| 原版 RA2 / YR 游戏资源 | 尚未找到，需要本地资源目录，才能验证零售地图和游戏启动 |
| 真机密钥 | 已取得并存入本机私有目录；正式 NSP 已构建、校验并通过核心真机运行测试 |

依赖检查可以复现：

```bash
# 包含桌面窗口栈：当前预期失败，显示实际的平台缺口
python3 build.py --check-client-deps

# 不包含 winit，按 Switch 原生前端选择 Vulkan-only：当前通过
python3 build.py --check-client-deps --without-winit
```

检查从桌面 Cargo.toml 读取依赖，并锁定桌面 Cargo.lock 中的对应版本。
上游的 libc、memmap2、wgpu 27、raw-window-handle、cpal 补丁仅通过生成的 Switch
配置启用，不修改桌面 Cargo.lock。补丁源码和许可来源见 `UPSTREAM.md`。

当前堆内存后备映射会把 MIX 整包读入内存；真实资源加载后还需要测量峰值内存。
此探针不验证大型纹理图集、所有文件操作或真实对局性能。

下一阶段仍需完成最小 Vulkan 三角形；当前已能清屏呈现，尚有模拟器着色器兼容性阻塞。
生产客户端还需要接原生窗口/输入前端、原有 wgpu 渲染路径和音频。
需要原版资源后再执行真实地图加载、渲染和游戏操作验证。

## Git 代理

开发机已有全局 Git HTTP/HTTPS 代理，并设置了同名环境变量。检查配置时
只确认是否启用，不打印代理凭据：

```bash
git config --global --get http.proxy >/dev/null && echo "Git proxy configured"
```

无需把代理地址写进本工程；后续依赖检查默认继承当前 shell 环境。
