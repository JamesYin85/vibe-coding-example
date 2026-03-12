# 分类规则

脚本会先应用确定性规则，再按需用兼容 OpenAI 的模型做细化。

## 优先级

1. `important`
2. `general`
3. `useless`

如果一封邮件同时命中多条规则，保留优先级最高的分类。

## `important`（重要）

满足以下任一条件时，将邮件提升为 `important`：

- 邮箱本人或配置中的某个别名出现在 `To` 中。
- 标题或正文包含明确行动信号，例如 `请处理`、`请尽快`、`urgent`、`action required`，或者命中了配置的 `important_keywords`。
- 发件人地址在 `important_senders` 列表中。
- 正文里出现了基于 `aliases` 推导出来的 `@alias` 提及。

默认分值：

- 如果是直接发给本人，或者包含明确行动信号，默认分值为 `5`。
- 如果主要信号来自发件人或 `@提及`，默认分值为 `4`。

## `general`（一般）

当邮件不满足 `important` 或 `useless` 条件，但仍然明显与工作相关时，分类为 `general`：

- 邮箱本人或别名出现在 `Cc` 中。
- 标题或正文包含信息通知类语言，例如 `FYI`、`知悉`、`通知`、`会议纪要`，或命中了配置的 `general_keywords`。
- 没有更强的规则被触发，但邮件也不是明显的群发噪音。

默认分值：

- 明确的信息通知类邮件默认分值为 `3`。
- 当没有命中其他规则，仅作为兜底分类时，默认分值为 `2`。

## `useless`（无用）

当群发或订阅特征占主导时，将邮件分类为 `useless`：

- 发件人包含 `no-reply`、`noreply`、`mailer-daemon` 等片段，或命中了配置中的 `useless_senders`。
- 标题或正文包含 `unsubscribe`、`订阅`、`promotion`、`digest`、`newsletter`，或命中了配置中的 `useless_keywords`。

默认分值：

- `1`

## 模型保护规则

- 规则判定为 `important` 的邮件不能被模型降为 `useless`。
- 规则判定为 `useless` 的邮件必须保持 `useless`。
- 模型输出的分类必须限制在 `important | general | useless` 之内。
- 如果模型调用失败或返回了非法 JSON，应回退到规则分级并生成兜底摘要。
