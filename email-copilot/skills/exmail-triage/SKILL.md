---
name: exmail-triage
description: 构建、配置、验证和维护 Exmail IMAP 轮询工作流，将收件箱邮件分为重要、一般和无用三类，并生成结构化摘要报告。当 Codex 需要为腾讯企业邮箱 Exmail 或类似 IMAP 邮箱创建或排查定时邮件分级自动化时使用，包括 YAML 配置、分类规则、摘要格式、归档路径、调度器接入，以及在 OpenAI、GLM 等兼容 OpenAI 的模型提供方之间切换。
---

# Exmail 邮件分级

## 概述

这个 skill 用于构建或维护本地的腾讯企业邮箱 Exmail 邮件分级流程。流程会通过 IMAP 轮询 `INBOX`，对未读邮件先做确定性规则分类，再通过兼容 OpenAI 的大模型补充摘要，随后将原始 `.eml` 邮件归档到本地，并输出 JSON 与 Markdown 两种摘要报告。

## 工作流

请按以下顺序实施或排查：

1. 安装后先运行 `scripts/self_check.py`，在不访问真实网络的情况下验证本地 Python、报告生成、归档和状态库流程。
2. 使用 `scripts/validate_config.py` 校验 YAML 配置。
3. 如果登录失败，先用 `scripts/imap_login_check.py` 单独排查 IMAP 登录。
4. 如果登录问题仍不明确，运行 `scripts/imap_diagnose.py`，逐步检查 DNS、TCP、SSL、IMAP 欢迎语、登录和 `INBOX` 选择过程。
5. 如需调整分类行为，查看 [references/classification-rules.md](references/classification-rules.md)。
6. 在启用调度器之前，先运行 `scripts/run_digest.py --config /path/to/config.yaml --dry-run` 预览结果。
7. 只有在 dry-run 输出符合预期后，才接入外部调度器。
8. 如果用户需要 `cron` 或 `launchd` 接入方式，查看 [references/scheduler-examples.md](references/scheduler-examples.md)。

## 常用命令

校验配置：

```bash
python3 skills/exmail-triage/scripts/validate_config.py --config /path/to/config.yaml
```

安装后运行离线自检：

```bash
python3 skills/exmail-triage/scripts/self_check.py
```

仅检查 IMAP 登录：

```bash
python3 skills/exmail-triage/scripts/imap_login_check.py --config /path/to/config.yaml
```

运行详细的 IMAP 诊断：

```bash
python3 skills/exmail-triage/scripts/imap_diagnose.py --config /path/to/config.yaml
```

预览一次轮询运行，不写归档和状态：

```bash
python3 skills/exmail-triage/scripts/run_digest.py --config /path/to/config.yaml --dry-run
```

处理真实邮件并写出报告：

```bash
python3 skills/exmail-triage/scripts/run_digest.py --config /path/to/config.yaml
```

限制调试运行范围：

```bash
python3 skills/exmail-triage/scripts/run_digest.py --config /path/to/config.yaml --limit 10 --since-minutes 30
```

## 行为约定

- 通过 IMAP 读取 `INBOX`，只搜索 `UNSEEN` 未读邮件。
- 保持邮箱状态不变。脚本不会将邮件标记为已读，也不会移动到其他文件夹。
- 使用本地 SQLite 状态库，对相同 `mailbox + uidvalidity + uid` 的邮件做去重处理。
- 将原始邮件按 `.eml` 格式保存到配置的归档目录。
- 每次非 dry-run 运行都会生成一份 Markdown 报告和一份 JSON 报告。
- 附件不在本 skill 的处理范围内。分类和摘要只使用邮件头部与正文文本。

## 配置规则

- 仅使用 YAML 配置。必填字段与示例请查看 [references/config.md](references/config.md)。
- 密码和 API Key 必须放在环境变量中，不要把密钥直接写进 YAML 文件。
- `aliases` 应只包含可能出现在 `To`、`Cc` 或 `@提及` 中的邮箱别名或短标识。
- 优先通过 `important_senders`、`important_keywords`、`general_keywords`、`useless_senders` 和 `useless_keywords` 调整规则，而不是直接改脚本。

## 模型规则

- 脚本会先执行规则分级，再调用兼容 OpenAI 的大模型生成 JSON 摘要并做有限细化。
- 新配置优先使用 `llm_provider`、`llm_api_key_env_var` 和 `llm_base_url`。旧的 `openai_*` 字段仍然兼容。
- 面向国内部署时，优先使用兼容 GLM 的配置，例如 `llm_provider: glm`、`llm_api_key_env_var: ZAI_API_KEY`、`llm_base_url: https://open.bigmodel.cn/api/paas/v4`。
- 规则判定为 `important` 的邮件，模型最多可以下调到 `general`，不能降到 `useless`。
- 规则判定为 `useless` 的邮件必须保持 `useless`，模型只负责补充简短摘要。
- 如果模型调用失败，脚本仍需基于规则启发式输出兜底摘要。

## 资源

- `scripts/run_digest.py`：主 CLI 入口，负责轮询、分类、归档和报告输出。
- `scripts/imap_diagnose.py`：Exmail IMAP 分步诊断脚本，检查 DNS、TCP、SSL、欢迎语、登录和 `INBOX` 选择。
- `scripts/imap_login_check.py`：最小化 IMAP 登录与邮箱检查脚本，用于快速排障。
- `scripts/self_check.py`：离线 smoke test，用于安装后验证和发布前校验。
- `scripts/validate_config.py`：配置校验脚本，可选输出规范化后的配置预览。
- `scripts/triage_lib.py`：共享库，封装 IMAP、解析、规则、模型、归档和报告逻辑。
- [references/config.md](references/config.md)：配置字段说明与示例 YAML。
- [references/classification-rules.md](references/classification-rules.md)：基础分类规则与优先级说明。
- [references/scheduler-examples.md](references/scheduler-examples.md)：外部调度器 `cron` 和 `launchd` 的示例配置。

## 输出约定

每条报告项应包含以下字段：

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

Markdown 摘要会按 `重要邮件`、`一般邮件` 和 `无用邮件` 三组输出。

## 排障建议

- 如果配置校验失败，先修复 YAML，不要在配置未通过前继续排查 IMAP 或模型调用。
- 如果 IMAP 登录成功但没有抓到邮件，确认 `INBOX` 中确实存在未读邮件。
- 如果模型调用失败，摘要仍应产出，并在 `reason_tags` 中包含 `model-fallback`。
- 如果重复邮件持续出现，检查 SQLite 状态库路径，并确认调度器不是以 dry-run 模式运行。
