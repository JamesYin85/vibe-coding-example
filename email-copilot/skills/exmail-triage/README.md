# Exmail Triage 使用说明

`exmail-triage` 是一个用于腾讯企业邮箱 Exmail 的本地邮件分级与摘要工具。它会通过 `IMAP` 读取收件箱中的未读邮件，按规则和模型能力对邮件进行分级，并输出结构化摘要报告。

当前实现重点是：

- 定期读取 `INBOX` 中的未读邮件
- 将邮件分为 `重要邮件`、`一般邮件`、`无用邮件`
- 为每封邮件生成简要摘要
- 将原始邮件归档为本地 `.eml` 文件
- 输出一份 `JSON` 报告和一份 `Markdown` 报告
- 使用本地 `SQLite` 记录已处理邮件，避免重复摘要

当前版本支持通过 OpenAI 兼容协议接入不同 LLM 提供方：

- OpenAI
- GLM

如果你在国内使用，建议优先配置 GLM。

这份 README 面向实际使用者，介绍如何安装、配置、运行、接入定时任务，以及如何理解输出结果。

## 1. 功能概览

这个 skill 主要解决的问题是：

- 你不想手动反复刷企业邮箱
- 你希望把真正需要处理的邮件优先提出来
- 你希望对普通通知类邮件做简明摘要
- 你希望把订阅、营销、系统广播类邮件尽量过滤掉
- 你希望每次执行都得到一份清晰的汇总结果

当前版本的能力边界如下：

- 只读取 `INBOX`
- 只处理 `未读邮件`
- 不会自动标记已读
- 不会自动移动邮件到其他文件夹
- 不会解析附件内容
- 不会生成 Exmail Web 端直达链接
- 原文访问地址使用本地归档 `.eml` 路径

## 2. 目录结构

当前 skill 目录结构如下：

```text
skills/exmail-triage/
├── README.md
├── SKILL.md
├── config.example.yaml
├── agents/
│   └── openai.yaml
├── references/
│   ├── classification-rules.md
│   ├── config.md
│   └── scheduler-examples.md
└── scripts/
    ├── imap_diagnose.py
    ├── imap_login_check.py
    ├── run_digest.py
    ├── self_check.py
    ├── triage_lib.py
    └── validate_config.py
```

各文件作用：

- `README.md`
  - 面向人类使用者的详细说明
- `SKILL.md`
  - 面向 Codex 的 skill 触发说明和工作流说明
- `scripts/imap_diagnose.py`
  - 分步诊断 Exmail IMAP 的 DNS、TCP、SSL、IMAP 握手、登录和 `INBOX` 选择过程
- `scripts/imap_login_check.py`
  - 只验证 Exmail IMAP 登录和 `INBOX` 访问，不执行摘要与模型请求
- `scripts/run_digest.py`
  - 主入口脚本，负责拉取邮件、分类、摘要、归档、写报告
- `scripts/self_check.py`
  - skill 内置离线自检脚本，适合发布后验证安装是否正确
- `scripts/validate_config.py`
  - 配置校验脚本，用于检查 YAML 配置是否完整、环境变量是否存在
- `scripts/triage_lib.py`
  - 共享逻辑，包括 IMAP 连接、邮件解析、规则分类、模型调用、报告生成、状态库管理
- `references/config.md`
  - 配置字段参考
- `references/classification-rules.md`
  - 分类规则说明
- `references/scheduler-examples.md`
  - `cron` 与 `launchd` 调度示例

## 3. 工作原理

每次运行时，脚本会按以下流程执行：

1. 读取 YAML 配置文件
2. 校验配置是否完整
3. 从环境变量中读取邮箱密码和 LLM API Key
4. 通过 IMAP 连接 Exmail 邮箱
5. 选择 `INBOX`
6. 搜索 `UNSEEN` 邮件
7. 用本地 SQLite 状态库过滤已处理过的 UID
8. 拉取邮件原文并解析发件人、收件人、抄送、主题、正文
9. 先使用规则做一轮分级
10. 对非无用邮件调用兼容 OpenAI 协议的 LLM 生成摘要并辅助修正等级
11. 将原始邮件保存为 `.eml`
12. 输出 JSON 和 Markdown 报告
13. 将本轮已处理邮件记录到本地 SQLite

注意：

- `--dry-run` 模式下不会写入 `.eml`、不会写 SQLite、不会写 Markdown/JSON 文件，而是直接把结果以 JSON 打印到标准输出
- 正式运行模式下会写报告和归档文件

## 4. 运行环境要求

建议使用：

- macOS 或 Linux
- Python 3.11 及以上
- 可访问腾讯企业邮箱 IMAP
- 可访问 OpenAI 兼容接口

当前脚本的 Python 外部依赖很少：

- `PyYAML`

安装方式示例：

```bash
python3 -m pip install pyyaml
```

说明：

- 当前版本没有依赖 OpenAI Python SDK，调用模型使用的是标准库 `urllib`
- 当前版本没有使用 `beautifulsoup4`
- 当前版本通过 OpenAI 兼容接口访问模型，因此可以接入 OpenAI 或 GLM

## 5. 快速开始

### 5.1 准备环境变量

先把邮箱密码和 LLM API Key 放到环境变量中。

示例：

```bash
export EXMAIL_PASSWORD='your-exmail-password'
export ZAI_API_KEY='your-glm-api-key'
```

如果你使用 OpenAI，则改为：

```bash
export OPENAI_API_KEY='your-openai-api-key'
```

当前脚本统一使用通用 LLM 配置字段：

- `llm_provider`
- `llm_api_key_env_var`
- `llm_base_url`

旧字段 `openai_api_key_env_var` 和 `openai_base_url` 仍兼容，但新配置不再推荐继续使用。

### 5.2 新建配置文件

在任意你方便的位置新建一个配置文件，比如：

```text
/Users/yourname/exmail-triage/config.yaml
```

你也可以直接从仓库里的示例文件开始：

```bash
cp skills/exmail-triage/config.example.yaml /Users/yourname/exmail-triage/config.yaml
```

推荐示例：

```yaml
imap_host: "imap.exmail.qq.com"
imap_port: 993
imap_ssl: true
email_address: "me@company.com"
password_env_var: "EXMAIL_PASSWORD"
interval_minutes: 15
llm_provider: "glm"
llm_api_key_env_var: "ZAI_API_KEY"
llm_base_url: "https://open.bigmodel.cn/api/paas/v4"
model: "glm-5"
display_name: "工作邮箱摘要"
aliases:
  - "me@company.com"
  - "my.alias@company.com"
  - "james"
important_senders:
  - "ceo@company.com"
  - "pm@company.com"
important_keywords:
  - "请处理"
  - "urgent"
  - "action required"
  - "请尽快"
general_keywords:
  - "FYI"
  - "知悉"
  - "会议纪要"
  - "通知"
useless_senders:
  - "newsletter@vendor.com"
  - "noreply"
  - "no-reply"
useless_keywords:
  - "unsubscribe"
  - "订阅"
  - "promotion"
  - "digest"
output_dir: "/Users/yourname/exmail-triage/reports"
archive_dir: "/Users/yourname/exmail-triage/archive"
state_db_path: "/Users/yourname/exmail-triage/state/triage.sqlite3"
timezone: "Asia/Shanghai"
max_emails_per_run: 50
```

### 5.3 校验配置

先验证配置，不要一上来就直接跑正式任务。

```bash
python3 skills/exmail-triage/scripts/validate_config.py \
  --config /Users/yourname/exmail-triage/config.yaml
```

如果你想看归一化后的配置内容：

```bash
python3 skills/exmail-triage/scripts/validate_config.py \
  --config /Users/yourname/exmail-triage/config.yaml \
  --show
```

校验通过时会输出：

```text
Config is valid.
```

### 5.3.1 单独验证 Exmail 登录

如果你怀疑问题出在邮箱登录，而不是摘要流程，可以先只跑 IMAP 登录检查：

```bash
python3 skills/exmail-triage/scripts/imap_login_check.py \
  --config /Users/yourname/exmail-triage/config.yaml
```

成功时会输出类似：

```text
IMAP login check passed.
Mailbox: INBOX
UIDVALIDITY: 123456789
```

这个脚本只验证：

- 配置可读取
- 环境变量存在
- Exmail IMAP 能登录
- `INBOX` 能正常选择

它不会：

- 拉取邮件
- 调用 GLM 或 OpenAI
- 写报告
- 写状态库

因此这是排查 Exmail 登录失败时的首选命令。

### 5.3.2 分步诊断 Exmail IMAP

如果 `imap_login_check.py` 仍然只给出“登录失败”，但你想知道具体卡在：

- 域名解析
- TCP 连通
- SSL 握手
- IMAP greeting
- IMAP 登录
- `INBOX` 选择

可以执行：

```bash
python3 skills/exmail-triage/scripts/imap_diagnose.py \
  --config /Users/yourname/exmail-triage/config.yaml
```

它会按步骤输出类似：

```text
[1/6] Resolving imap.exmail.qq.com
[2/6] Opening TCP connection to imap.exmail.qq.com:993
[3/6] Performing SSL handshake
[4/6] Opening IMAP session and reading server greeting
[5/6] Authenticating with IMAP LOGIN
[6/6] Selecting INBOX
```

这样你就能知道故障到底出在网络、SSL 还是账号认证。

### 5.4 先做干跑

强烈建议先使用 `--dry-run` 检查行为是否符合预期：

```bash
python3 skills/exmail-triage/scripts/run_digest.py \
  --config /Users/yourname/exmail-triage/config.yaml \
  --dry-run
```

此模式下：

- 会连接邮箱
- 会抓取候选邮件
- 会做分类与摘要
- 不会写入归档
- 不会写入状态库
- 不会写报告文件
- 会将结果直接打印为 JSON

### 5.5 正式执行

确认干跑结果合理后，再做正式执行：

```bash
python3 skills/exmail-triage/scripts/run_digest.py \
  --config /Users/yourname/exmail-triage/config.yaml
```

执行成功时会输出类似：

```text
Wrote JSON report to /Users/yourname/exmail-triage/reports/2026-03-12/101530-report.json
Wrote Markdown report to /Users/yourname/exmail-triage/reports/2026-03-12/101530-report.md
```

## 5.6 初次安装如何验证是否正确

首次安装后，建议按“离线自检 -> 配置校验 -> 干跑 -> 正式执行”四步验证。

### 第一步：离线自检

这个步骤不依赖真实 Exmail 账号，也不依赖真实 OpenAI 请求。它主要验证：

- Python 运行正常
- skill 文件结构完整
- 入口脚本可导入
- 报告写入链路正常
- 状态库写入链路正常
- 归档 `.eml` 写入链路正常

如果你发布到 OpenClaw 后，接收方只拿到了 skill 目录本身，优先运行 skill 自带的离线自检：

```bash
python3 skills/exmail-triage/scripts/self_check.py
```

预期结果类似：

```text
[1/3] Running dry-run pipeline check
[2/3] Running live-write pipeline check
[3/3] Verifying output structure
Self-check passed.
```

这个命令不依赖仓库根目录下的 `tests/` 和 `Makefile`，更适合用于 OpenClaw 发布后的安装验证。

执行命令：

```bash
make smoke-test
```

或者：

```bash
python3 -m unittest discover -s tests/integration -p 'test_*.py'
```

预期结果：

```text
Ran 2 tests in ...
OK
```

如果这个步骤失败，说明安装本身就还没有正确，不要继续调 Exmail 或 OpenAI。

### 第二步：配置校验

这个步骤验证：

- 配置文件格式正确
- 必填字段齐全
- 环境变量存在
- 路径可用
- 时区合法

执行命令：

```bash
python3 skills/exmail-triage/scripts/validate_config.py \
  --config /Users/yourname/exmail-triage/config.yaml
```

预期结果：

```text
Config is valid.
```

### 第三步：真实邮箱干跑

这个步骤会真实连接邮箱，但不会写报告文件和状态库。

执行命令：

```bash
python3 skills/exmail-triage/scripts/run_digest.py \
  --config /Users/yourname/exmail-triage/config.yaml \
  --dry-run
```

你需要重点确认：

- 能成功连接 IMAP
- 能抓到未读邮件
- 返回的是合法 JSON
- 邮件分级基本符合预期
- 摘要内容可接受

如果干跑输出为空或没有抓到邮件，先检查：

- `INBOX` 是否真的有未读邮件
- IMAP 是否开启
- 环境变量是否正确
- `--since-minutes` 是否过滤过严

### 第四步：正式执行

这个步骤才会真正写文件。

执行命令：

```bash
python3 skills/exmail-triage/scripts/run_digest.py \
  --config /Users/yourname/exmail-triage/config.yaml
```

你需要确认三类输出是否都出现：

1. 报告文件
2. 原始邮件归档文件
3. SQLite 状态库

检查方式：

```bash
find /Users/yourname/exmail-triage/reports -type f
find /Users/yourname/exmail-triage/archive -type f
ls -l /Users/yourname/exmail-triage/state/triage.sqlite3
```

正式执行通过的最小判断标准是：

- 生成了 `*-report.json`
- 生成了 `*-report.md`
- 生成了对应的 `.eml`
- 生成了 `triage.sqlite3`

### 第五步：重复执行验证去重是否正常

正式执行后，再运行一次相同命令：

```bash
python3 skills/exmail-triage/scripts/run_digest.py \
  --config /Users/yourname/exmail-triage/config.yaml
```

如果状态库正常工作，那么同一批已处理未读邮件不应该被重复摘要。

你可以通过查看新生成的 JSON 报告中的：

- `processed_count`
- `items`

来判断是否重复处理。

### 推荐验证顺序

首次安装后，推荐按下面顺序执行：

1. `make lint`
2. `make test`
3. `make smoke-test`
4. `validate_config.py`
5. `run_digest.py --dry-run`
6. `run_digest.py`

其中：

- `make lint` 用于验证 skill 结构和 Python 可编译性
- `make test` 用于运行单元测试和集成测试
- `make smoke-test` 用于做离线冒烟自检
- `validate_config.py` 用于验证你自己的配置和环境变量
- `run_digest.py --dry-run` 用于验证真实邮箱读取与分类输出
- `run_digest.py` 用于验证正式落盘结果

## 6. 配置文件详解

下面对每个配置字段做更详细说明。

### 6.1 IMAP 连接相关

- `imap_host`
  - 企业邮箱 IMAP 主机
  - 腾讯企业邮箱通常为 `imap.exmail.qq.com`
- `imap_port`
  - 通常使用 `993`
- `imap_ssl`
  - 是否启用 SSL
  - 一般应设为 `true`
- `email_address`
  - 邮箱主地址
- `password_env_var`
  - 邮箱密码所在环境变量名，不是密码本身

### 6.2 调度与模型相关

- `interval_minutes`
  - 轮询间隔，供调度器参考
  - 脚本本身不会自动 sleep，也不会自己常驻运行
- `llm_provider`
  - LLM 提供方标识
  - 建议值：
    - `openai`
    - `glm`
- `llm_api_key_env_var`
  - LLM API Key 所在环境变量名
- `model`
  - 模型名称
- `llm_base_url`
  - 可选
  - 当你使用兼容 OpenAI 的中转接口时可设置
  - 可以填写完整 `/chat/completions` 地址
  - 也可以只填写兼容 API 根地址，如 `/v1` 或 `/v4`

兼容性说明：

- 新配置推荐使用：
  - `llm_provider`
  - `llm_api_key_env_var`
  - `llm_base_url`
- 旧配置仍兼容：
  - `openai_api_key_env_var`
  - `openai_base_url`

### 6.2.1 国内 GLM 推荐配置

如果你在国内使用，推荐优先采用 GLM 配置：

```yaml
llm_provider: "glm"
llm_api_key_env_var: "ZAI_API_KEY"
llm_base_url: "https://open.bigmodel.cn/api/paas/v4"
model: "glm-5"
```

说明：

- `llm_base_url` 可以写到 `/v4`
- 脚本会自动补到 `/chat/completions`
- 如果你填写完整的 `/chat/completions` 地址，也可以正常工作

### 6.2.2 OpenAI 配置示例

```yaml
llm_provider: "openai"
llm_api_key_env_var: "OPENAI_API_KEY"
llm_base_url: "https://api.openai.com/v1"
model: "gpt-4.1-mini"
```

### 6.3 身份识别相关

- `display_name`
  - 报告标题展示名
- `aliases`
  - 用来识别“是否发给我”“是否 @ 我”的额外身份标识
  - 可以放：
    - 主邮箱地址
    - 别名邮箱
    - 英文名
    - 常用短用户名

建议：

- 如果别人常在正文中写 `@james`、`@alex` 这种形式，可以把 `james`、`alex` 放到 `aliases`
- 如果只写邮箱地址，不需要放太多昵称

### 6.4 分类规则相关

- `important_senders`
  - 精确发件人地址
  - 命中后更容易被判为重要邮件
- `important_keywords`
  - 重要邮件关键词
  - 例如：`请处理`、`urgent`
- `general_keywords`
  - 一般邮件关键词
  - 例如：`FYI`、`知悉`、`会议纪要`
- `useless_senders`
  - 无用邮件发件人片段
  - 例如：`noreply`、`newsletter`
- `useless_keywords`
  - 无用邮件关键词
  - 例如：`unsubscribe`、`订阅`

### 6.5 输出与状态相关

- `output_dir`
  - 报告输出目录
- `archive_dir`
  - 原始邮件 `.eml` 归档目录
- `state_db_path`
  - SQLite 状态库文件路径
- `timezone`
  - 报告时间戳和邮件时间转换使用的时区
- `max_emails_per_run`
  - 每次最多处理多少封邮件

## 7. 命令行参数说明

`run_digest.py` 支持以下参数：

### `--config`

必填参数。

示例：

```bash
--config /Users/yourname/exmail-triage/config.yaml
```

### `--dry-run`

可选参数。

作用：

- 不写文件
- 不写状态库
- 直接输出 JSON

适用场景：

- 首次联调
- 调整关键词后验证效果
- 排查模型输出

### `--limit`

可选参数。

作用：

- 限制本次最多处理多少封邮件

示例：

```bash
python3 skills/exmail-triage/scripts/run_digest.py \
  --config /Users/yourname/exmail-triage/config.yaml \
  --limit 10
```

### `--since-minutes`

可选参数。

作用：

- 只处理最近 N 分钟内收到的邮件

示例：

```bash
python3 skills/exmail-triage/scripts/run_digest.py \
  --config /Users/yourname/exmail-triage/config.yaml \
  --since-minutes 30
```

这个参数更适合调试，而不是长期依赖。

## 8. 分级规则说明

当前分级为三类：

- `important`
- `general`
- `useless`

Markdown 报告中会显示成：

- `重要邮件`
- `一般邮件`
- `无用邮件`

### 8.1 重要邮件

以下情况更容易被判为重要：

- 你在 `To` 中
- 发件人在 `important_senders`
- 邮件主题或正文命中重要关键词
- 正文中出现与你别名相关的 `@mention`

例如：

- 主题：`请处理：客户升级问题`
- 收件人：你本人
- 发件人：直属上级

### 8.2 一般邮件

以下情况更容易被判为一般：

- 你在 `Cc` 中
- 内容更像通知、知会、周报、纪要
- 没有强烈的操作要求，但与工作相关

例如：

- 主题：`FYI：本周项目周报`
- 你在抄送中

### 8.3 无用邮件

以下情况更容易被判为无用：

- 发件人类似 `no-reply`、`noreply`、`mailer-daemon`
- 主题或正文包含订阅、营销、退订等特征
- 明显是批量通知或促销内容

例如：

- 主题：`Newsletter Digest`
- 正文中有 `unsubscribe`

### 8.4 规则与模型的关系

当前实现采用“规则优先，模型辅助”策略：

- 先通过规则做第一轮判断
- 再调用兼容 OpenAI 协议的 LLM 生成摘要和辅助修正
- 规则判定为 `important` 的邮件，不允许被模型降级成 `useless`
- 规则判定为 `useless` 的邮件，会保持为 `useless`
- 如果模型调用失败，会回退到规则摘要

## 9. 输出结果说明

每次正式执行会输出两份文件：

- 一份 JSON
- 一份 Markdown

### 9.1 报告目录结构

JSON 和 Markdown 报告会写入：

```text
output_dir/YYYY-MM-DD/HHMMSS-report.json
output_dir/YYYY-MM-DD/HHMMSS-report.md
```

例如：

```text
/Users/yourname/exmail-triage/reports/2026-03-12/101530-report.json
/Users/yourname/exmail-triage/reports/2026-03-12/101530-report.md
```

### 9.2 原始邮件归档结构

原始 `.eml` 文件会写入：

```text
archive_dir/YYYY/MM/DD/<uid>.eml
```

例如：

```text
/Users/yourname/exmail-triage/archive/2026/03/12/42.eml
```

### 9.3 JSON 报告字段

JSON 顶层结构包含：

- `generated_at`
- `mailbox`
- `processed_count`
- `counts`
- `items`

每个 `items` 条目包含：

- `subject`
- `from_name`
- `from_address`
- `received_at`
- `classification`
- `importance_score`
- `action_required`
- `reason_tags`
- `summary`
- `excerpt`
- `message_id`
- `archived_path`

示例：

```json
{
  "subject": "请处理：客户报价审批",
  "from_name": "Alice",
  "from_address": "alice@company.com",
  "received_at": "2026-03-12T09:30:00+08:00",
  "classification": "important",
  "importance_score": 5,
  "action_required": true,
  "reason_tags": [
    "direct-recipient",
    "important-keyword:请处理"
  ],
  "summary": "Alice 发来客户报价审批邮件，需要尽快确认审批结论。",
  "excerpt": "请今天中午前处理客户 A 的报价审批，附件已更新。",
  "message_id": "<abc123@company.com>",
  "archived_path": "/Users/yourname/exmail-triage/archive/2026/03/12/42.eml"
}
```

### 9.4 Markdown 报告结构

Markdown 报告会按分组输出：

- `## 重要邮件`
- `## 一般邮件`
- `## 无用邮件`

每封邮件会包含：

- 标题
- 发件人
- 接收时间
- 重要程度
- 是否需要处理
- 原文路径
- 标签
- 摘要
- 摘录

## 10. 本地状态库说明

为了避免重复处理同一封邮件，脚本会把处理结果记录到 SQLite 中。

去重主键逻辑为：

- `mailbox`
- `uidvalidity`
- `uid`

这意味着：

- 同一个邮箱同一个 UID 不会被重复摘要
- 即使邮件还保持未读，也不会在后续重复出现在报告中
- 如果邮箱端 UID 体系变化，仍有机会重新进入处理流程

注意：

- 如果你删除了 `state_db_path` 指向的 SQLite 文件，系统会重新处理历史未读邮件
- 如果你经常使用 `--dry-run`，它不会写入状态库，因此下次仍会重新看到这些邮件

## 11. 如何接入定时任务

当前 skill 不会自己常驻运行。你需要用外部调度器定期触发。

### 11.1 使用 cron

示例：

```cron
*/15 * * * * cd /path/to/repo && EXMAIL_PASSWORD=... ZAI_API_KEY=... python3 skills/exmail-triage/scripts/run_digest.py --config /path/to/config.yaml >> /tmp/exmail-triage.log 2>&1
```

适用场景：

- Linux 服务器
- macOS 也可用，但通常更推荐 `launchd`

### 11.2 使用 macOS launchd

如果你在 macOS 上长期使用，建议用 `launchd`。

可参考：

[references/scheduler-examples.md](references/scheduler-examples.md)

原因：

- 和系统集成更稳定
- 适合用户级后台任务
- 更符合 macOS 的原生调度方式

### 11.3 调度频率建议

建议调度器执行频率与配置中的 `interval_minutes` 保持一致。

例如：

- `interval_minutes: 15`
- 调度器就每 15 分钟执行一次

注意：

- `interval_minutes` 只是配置约定和文档字段
- 脚本本身不会根据这个字段自动休眠或循环

## 12. 常见使用场景

### 场景 1：每天收大量抄送邮件

做法：

- 把常见通知关键词加到 `general_keywords`
- 把关键领导或直接协作者加入 `important_senders`
- 用 `--dry-run` 观察分类是否合理

### 场景 2：经常收到订阅和系统广播

做法：

- 在 `useless_senders` 中加入 `noreply`、`no-reply`
- 在 `useless_keywords` 中加入 `unsubscribe`、`digest`、`newsletter`

### 场景 3：正文中经常出现 `@英文名`

做法：

- 把英文名加入 `aliases`
- 例如 `james`

这样脚本更容易识别“@我处理”类邮件。

### 场景 4：只想临时检查最近半小时的未读邮件

做法：

```bash
python3 skills/exmail-triage/scripts/run_digest.py \
  --config /Users/yourname/exmail-triage/config.yaml \
  --dry-run \
  --since-minutes 30 \
  --limit 20
```

## 13. 排障指南

### 13.1 配置校验失败

常见原因：

- 缺少必填字段
- 路径为空
- 时区不合法
- 环境变量不存在

优先执行：

```bash
python3 skills/exmail-triage/scripts/validate_config.py \
  --config /Users/yourname/exmail-triage/config.yaml \
  --show
```

### 13.2 IMAP 登录失败

检查：

- `imap_host` 是否正确
- `imap_port` 是否正确
- 邮箱密码是否正确
- 企业邮箱是否允许 IMAP
- 网络是否可以访问企业邮箱 IMAP 服务

如果这些还不能定位问题，先运行：

```bash
python3 skills/exmail-triage/scripts/imap_diagnose.py \
  --config /Users/yourname/exmail-triage/config.yaml
```

然后根据失败停留的步骤判断：

- 卡在 `[1/6]`
  - 域名解析问题
- 卡在 `[2/6]`
  - TCP 连通问题、防火墙或端口问题
- 卡在 `[3/6]`
  - SSL 握手问题
- 卡在 `[4/6]`
  - IMAP 服务握手异常
- 卡在 `[5/6]`
  - 账号、密码、IMAP 开通状态或风控问题
- 卡在 `[6/6]`
  - 邮箱文件夹访问问题

### 13.3 没有抓到任何邮件

检查：

- 是否真的有 `INBOX` 中的未读邮件
- 邮件是否已经被状态库标记为处理过
- 是否使用了 `--since-minutes` 导致全部被过滤
- 是否 `--limit` 过小

### 13.4 摘要质量不理想

调整方向：

- 补充 `important_keywords`
- 补充 `general_keywords`
- 补充 `useless_keywords`
- 调整 `important_senders`
- 调整 `aliases`

如果模型调用失败，结果会退化为规则摘要，这时通常会在 `reason_tags` 中看到：

- `model-fallback`

### 13.5 重复处理邮件

检查：

- 你是否一直在用 `--dry-run`
- `state_db_path` 是否指向稳定的持久路径
- 调度任务是否在多个不同环境中重复执行

### 13.6 输出目录没有文件

检查：

- 是否使用了 `--dry-run`
- `output_dir` 和 `archive_dir` 是否有写权限
- 正式运行时是否抛出了异常

## 14. 当前限制

当前版本有这些明确限制：

- 只支持 `INBOX`
- 只支持读取 `UNSEEN`
- 不读取附件正文
- 不做 OCR
- 不支持企业微信消息推送
- 不支持再次发邮件回传摘要
- 不支持直接生成 Webmail 原文链接
- 不支持自动修改邮箱状态
- 不提供守护进程

如果后续要扩展，优先可以考虑：

- 解析文本附件
- 推送摘要到企业微信
- 支持多文件夹轮询
- 支持多账号
- 支持日报聚合
- 支持更细粒度的优先级

## 15. 推荐使用流程

建议按下面顺序上线：

1. 安装 `PyYAML`
2. 设置环境变量
3. 创建配置文件
4. 运行 `validate_config.py`
5. 运行一次 `run_digest.py --dry-run`
6. 调整关键词和发件人规则
7. 运行正式任务
8. 检查 JSON、Markdown、`.eml` 是否符合预期
9. 再接入 `cron` 或 `launchd`

## 16. 相关文件

- Skill 说明：
  - [SKILL.md](./SKILL.md)
- 示例配置：
  - [config.example.yaml](./config.example.yaml)
- IMAP 登录排查：
  - [scripts/imap_login_check.py](./scripts/imap_login_check.py)
- IMAP 分步诊断：
  - [scripts/imap_diagnose.py](./scripts/imap_diagnose.py)
- 内置自检：
  - [scripts/self_check.py](./scripts/self_check.py)
- 配置参考：
  - [references/config.md](./references/config.md)
- 分类规则：
  - [references/classification-rules.md](./references/classification-rules.md)
- 调度示例：
  - [references/scheduler-examples.md](./references/scheduler-examples.md)

## 17. 一条最小可行命令

如果你已经准备好环境变量和配置文件，最小可行命令是：

```bash
python3 skills/exmail-triage/scripts/run_digest.py \
  --config /Users/yourname/exmail-triage/config.yaml
```

如果你是第一次使用，最小安全命令是：

```bash
python3 skills/exmail-triage/scripts/run_digest.py \
  --config /Users/yourname/exmail-triage/config.yaml \
  --dry-run
```
