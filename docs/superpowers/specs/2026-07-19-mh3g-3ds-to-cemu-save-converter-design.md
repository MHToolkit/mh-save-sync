# 日版 MH3G 3DS 到 Cemu 存档转换器设计

**状态：** 已确认，待实现计划

## 目标与范围

新增一个离线 Rust CLI，将日版《Monster Hunter 3G》的 3DS/Nemessix/Azahar 单槽存档转换为日版《Monster Hunter 3G HD Ver.》的 Cemu 单槽存档。

第一版的唯一支持路径：

```text
3DS/Nemessix/Azahar: title/00040000/00048100/data/00000001/user[1-3]
    ->
Cemu: usr/save/00050000/10104D00/user/80000001/user[1-3]
```

第一版不支持其他区域、其他游戏、Wii U 到 3DS 的反向转换，或同步服务中的自动转换。

## 已验证格式事实

| 项目 | 日版 3DS 源 | 日版 Cemu 目标 |
| --- | --- | --- |
| 单槽文件长度 | `0x8A00` | `0x8A24` |
| 主体前缀 | 4 字节 | 40 字节 |
| 日版版本值 | `0x2B` | 容器末字节 `0x2B` |
| 数据主体长度 | `0x89FC` | `0x89FC` |

本机样本已证明源档和日版 Cemu 的相关容器都使用 `0x2B`。因此不能直接使用参考项目 `fadillzzz/3usavetools` 的欧美 `0x2C` 目标头。

## 架构

在 Rust workspace 中新增 `crates/mh3g-save-convert`：

```text
crates/mh3g-save-convert/
  src/
    format.rs       # JP 3DS/Cemu 头与固定大小
    profile.rs      # 输入和输出 profile 验证
    transforms.rs   # 已知的端序和位域转换
    transaction.rs  # 快照、临时写入和回滚
    main.rs         # clap CLI
  tests/
```

该 crate 只依赖 workspace 的本地工具库和 Rust 标准库；不接入网络、服务端或同步 watcher。

## 转换数据流

1. 验证输入是常规文件，长度为 `0x8A00`，并且四字节头是日版 `0x2B` profile。
2. 检查 Nemessix/Azahar 与 Cemu 未运行；运行中即拒绝执行。
3. 读取源档，剥离 4 字节源容器头，得到 `0x89FC` 数据主体。
4. 复制主体，仅对已知字段应用转换：
   - 参考实现列出的 16/32 位整数端序区间；
   - 怪物图鉴旗标：`1/2/4/8` 到 `0x80/0x20/0x40/0x08`；
   - 斗技场记录的字节交换、位移与 dropped-bit 迁移。
5. 加入 40 字节日版 Cemu 头，尾部版本值固定为 `0x2B`。
6. 验证生成长度为 `0x8A24`，重新解析 profile，并检查所有变换范围都在主体边界内。
7. 仅在 `--write` 时执行安装事务；否则仅输出验证结果。

未在转换表中的主体字节必须原样保留。

## CLI

```bash
mh3g-save-convert inspect <source>
mh3g-save-convert convert <source> --output <user-slot> --dry-run
mh3g-save-convert convert <source> --output <user-slot> --write
mh3g-save-convert rollback --manifest <manifest>
```

`inspect` 只读。`convert` 不带 `--write` 时不改动 Cemu 目录。`--output` 必须是日版 Cemu MH3G HD 的 `user1`、`user2` 或 `user3` 槽位。

## 安全与回滚

- 源存档永不修改。
- Cemu 或源模拟器运行时拒绝安装。
- 目标存在时，先在同目录创建 `userN.mh3g-backup-<UTC 时间戳>`。
- 转换清单记录源、旧目标（若存在）和新目标的 SHA-256、长度、profile 与快照路径；不得记录存档内容。
- 在目标同一文件系统写入临时文件、校验后原子替换。
- 任一步失败时保留旧目标并清理临时文件。
- `rollback` 只接受本工具生成且校验通过的清单。

## 验证策略

### 单元测试

使用确定性合成 fixture 覆盖 profile 识别、长度/版本拒绝、所有变换、保留字节、目标头、失败原子性和回滚。

### 参考差分

将 `3usavetools 0.3.1` 用作转换规则参考。日版结果预期只与其欧美输出的版本字节 `0x2C -> 0x2B` 存在区域头差异，其他已知变换必须一致。

### 真实样本

仅从现有 Nemessix `user2` 创建工作目录快照，不提交真实存档。验证源哈希不变、目标长度和 profile 正确，并比较玩家资料、金钱、时间、道具箱、装备箱、Moga 点数、图鉴和公会卡相关字段。

### Cemu 端到端

在 Cemu 停止状态下，先快照目标槽位，再安装转换结果并启动日版 MH3G HD。验收要求游戏识别存档、可进入角色并检查关键数据；退出后文件仍可解析；最后验证 rollback 恢复安装前状态。

## 完成标准

- `cargo test -p mh3g-save-convert` 通过。
- `git diff --check` 通过。
- 日版 Cemu 可加载生成的 `user2`。
- 关键数据与源存档一致，源档哈希不变。
- 安装失败与 rollback 都能保持或恢复目标状态。
- 未做语义验证的字段明确标记为“字节保留，未语义验证”，不宣称全量无损。

## 参考

- fadillzzz/3usavetools `0.3.1`：容器长度、端序区间、怪物图鉴与斗技场转换规则。
- 本机日版 3DS/Cemu 样本：确认 `0x2B` 区域版本值、路径和容器长度。
