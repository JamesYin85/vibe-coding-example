# 配置说明

请使用 YAML 文件，并提供以下必填字段：

- `imap_host`：Exmail IMAP 主机，例如 `imap.exmail.qq.com`
- `imap_port`：通常为 `993`
- `imap_ssl`：是否启用 IMAP over SSL，通常为 `true`
- `email_address`：主邮箱地址
- `password_env_var`：保存邮箱密码的环境变量名
- `interval_minutes`：外部调度器使用的轮询间隔（分钟）
- `llm_provider`：模型提供方标识，例如 `openai` 或 `glm`
- `llm_api_key_env_var`：保存模型 API Key 的环境变量名
- `llm_base_url`：Chat Completions 接口地址，或兼容 OpenAI 的 API 根路径
- `model`：模型名称
- `display_name`：报告标题前缀
- `aliases`：可能出现在 `To`、`Cc` 或 `@提及` 中的邮箱别名或短标识
- `important_senders`：应偏向判定为 `important` 的精确发件人地址
- `important_keywords`：补充的 `important` 标题/正文关键词
- `general_keywords`：补充的 `general` 标题/正文关键词
- `useless_senders`：补充的 `useless` 发件人片段
- `useless_keywords`：补充的 `useless` 标题/正文关键词
- `output_dir`：Markdown 和 JSON 报告的根目录
- `archive_dir`：原始 `.eml` 邮件归档根目录
- `state_db_path`：用于已处理邮件跟踪的 SQLite 数据库路径
- `timezone`：IANA 时区名，例如 `Asia/Shanghai`
- `max_emails_per_run`：单次轮询允许处理的最大邮件数

可选字段：

- `openai_api_key_env_var`：`llm_api_key_env_var` 的兼容别名
- `openai_base_url`：`llm_base_url` 的兼容别名
- `provider`：`llm_provider` 的兼容别名

面向国内 GLM 部署的示例：

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
display_name: "工作邮箱"
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
general_keywords:
  - "FYI"
  - "知悉"
  - "会议纪要"
useless_senders:
  - "newsletter@vendor.com"
  - "noreply"
useless_keywords:
  - "unsubscribe"
  - "订阅"
  - "promotion"
output_dir: "/Users/you/exmail-digests/reports"
archive_dir: "/Users/you/exmail-digests/archive"
state_db_path: "/Users/you/exmail-digests/state/triage.sqlite3"
timezone: "Asia/Shanghai"
max_emails_per_run: 50
```

OpenAI 的示例：

```yaml
imap_host: "imap.exmail.qq.com"
imap_port: 993
imap_ssl: true
email_address: "me@company.com"
password_env_var: "EXMAIL_PASSWORD"
interval_minutes: 15
llm_provider: "openai"
llm_api_key_env_var: "OPENAI_API_KEY"
llm_base_url: "https://api.openai.com/v1"
model: "gpt-4.1-mini"
display_name: "工作邮箱"
aliases: []
important_senders: []
important_keywords: []
general_keywords: []
useless_senders: []
useless_keywords: []
output_dir: "/Users/you/exmail-digests/reports"
archive_dir: "/Users/you/exmail-digests/archive"
state_db_path: "/Users/you/exmail-digests/state/triage.sqlite3"
timezone: "Asia/Shanghai"
max_emails_per_run: 50
```

校验要求：

- 正式运行前，`password_env_var` 和 `llm_api_key_env_var` 对应的环境变量必须已经存在。
- `output_dir`、`archive_dir` 和 `state_db_path` 的父目录必须可写。
- `interval_minutes` 和 `max_emails_per_run` 必须大于零。
- `llm_base_url` 既可以是完整的 chat completions URL，也可以是以 `/v1` 或 `/v4` 结尾的兼容 OpenAI API 根路径；脚本会自动规范化。
