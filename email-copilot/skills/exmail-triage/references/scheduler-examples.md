# 调度器示例

这个 skill 不会替你安装调度器。下面的内容只作为接入示例。

## cron

每 15 分钟执行一次：

```cron
*/15 * * * * cd /path/to/repo && EXMAIL_PASSWORD=... ZAI_API_KEY=... python3 skills/exmail-triage/scripts/run_digest.py --config /path/to/config.yaml >> /tmp/exmail-triage.log 2>&1
```

说明：

- 如果不希望把密钥直接写在 crontab 中，请通过更安全的包装脚本或环境加载方式导出。
- 保持工作目录稳定，确保配置中的相对路径行为可预测。

## launchd

最小化 `LaunchAgent` 示例：

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
  <dict>
    <key>Label</key>
    <string>com.example.exmail-triage</string>
    <key>ProgramArguments</key>
    <array>
      <string>/usr/bin/env</string>
      <string>python3</string>
      <string>/path/to/repo/skills/exmail-triage/scripts/run_digest.py</string>
      <string>--config</string>
      <string>/path/to/config.yaml</string>
    </array>
    <key>StartInterval</key>
    <integer>900</integer>
    <key>WorkingDirectory</key>
    <string>/path/to/repo</string>
    <key>EnvironmentVariables</key>
    <dict>
      <key>EXMAIL_PASSWORD</key>
      <string>...</string>
      <key>ZAI_API_KEY</key>
      <string>...</string>
    </dict>
    <key>StandardOutPath</key>
    <string>/tmp/exmail-triage.out.log</string>
    <key>StandardErrorPath</key>
    <string>/tmp/exmail-triage.err.log</string>
  </dict>
</plist>
```

说明：

- `StartInterval` 的单位是秒，`900` 表示每 15 分钟执行一次。
- 优先使用 `launchctl bootstrap gui/$UID ~/Library/LaunchAgents/com.example.exmail-triage.plist`，不要再使用旧的 load 命令。
- `WorkingDirectory` 最好指向仓库根目录，以保证相对导入和相对路径稳定。
