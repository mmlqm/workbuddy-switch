# workbuddy-switch

WorkBuddy / CodeBuddy CLI / CodeBuddy CN IDE 账号切换桌面 App（Tauri），支持积分到期监控、自动签到、成长中心一键刷任务。

多账号共享登录态，一键切换 WorkBuddy 登录账号，并支持将当前账号的会话复制给目标账号（云端归属目标）。

<p align="center">
  <img src="public/icon-transparent.png" alt="WorkBuddy Switch 图标" width="128" />
</p>

<p align="center">
  <strong>workbuddy-switch</strong><br />
  WorkBuddy / CodeBuddy CLI 账号切换 + 成长中心自动挂机
</p>

## 快速开始

### 桌面 App

从 [GitHub Releases](https://github.com/mmlqm/workbuddy-switch/releases/latest) 下载对应平台安装包：

| 平台 | 安装包 |
| --- | --- |
| Windows x64 | `workbuddy-switch_<版本>_x64-setup.exe` |
| macOS Apple Silicon | `workbuddy-switch_<版本>_aarch64.dmg` |
| macOS Intel | `workbuddy-switch_<版本>_x86_64.dmg` |
| Linux x64 | `.deb` / `.AppImage` |

### 从源码运行

```bash
git clone https://github.com/mmlqm/workbuddy-switch.git
cd workbuddy-switch
npm install
npm run build
cargo build -p wb-switch-rust
```

## 功能

| 模块 | 说明 |
| --- | --- |
| 账号管理 | OAuth 扫码登录、从本机导入、手动添加 token、删除账号 |
| 账号切换 | 备份认证文件 → 关闭 WorkBuddy → 写入目标账号 → 重启，切换过程实时进度反馈 |
| 会话复制 | 将当前账号勾选的会话以新 id 复制给目标账号 |
| 自动签到 | 启动时立即检查，每 30 分钟自动补签；一键全部签到；签到日志 |
| **成长中心** | **成长计划任务一键挂机：接受任务、刷进度、领奖、大转盘抽奖、盲盒开箱；支持单账号执行和全部国内版账号批量执行** |
| Token 保活 | 操作前自动刷新 + 每日保活，避免 refresh token 过期 |
| 积分到期查询 | 自动查询每个账号的积分资源、剩余量和到期时间；临近到期高亮 |
| 积分统计 | 汇总官方请求用量，每日趋势、模型分布、账号消耗和请求明细 |
| Token 统计 | WorkBuddy / CodeBuddy CLI / IDE 的 Token 总览、趋势图、项目/模型排行 |
| CodeBuddy CLI | 复用同一账号库，默认账号独立；手动切换先关闭运行中的 CLI，立即生效 |
| CodeBuddy CN IDE | 向 CodeBuddy CN 桌面客户端注入凭证并重启 |
| 自动轮换 | 后台定时把 CodeBuddy CLI 默认账号切到积分最紧迫的账号 |
| 权限检测 | macOS 授权引导（App 管理 / 完全磁盘访问） |

## 使用

1. **添加账号**：账号页 →「扫码登录」或「从本机导入」
2. **切换账号**：账号卡片 →「切换」，可勾选复制当前会话
3. **成长中心**：左侧导航进入「成长中心」→ 选择账号 → 点「一键执行」自动完成接受任务/刷进度/领奖/抽奖/盲盒；或点「全部账号执行」批量处理所有国内版账号
4. **自动签到**：账号页直接开关；设置页可调保活参数
5. **积分/Token 统计**：侧栏进入对应页面查看趋势和明细
6. **CodeBuddy CLI/IDE**：账号卡片一键切换

## 成长中心说明

成长中心对接 WorkBuddy 成长计划活动，自动完成以下类型任务：

- **事件上报类**：创建画布、使用模板、Playbook、自动化任务、体验资料库、发现应用/企鹅教师助手
- **召唤专家类**：召唤 5 次专家、召唤 3 次专家团（自动补到所需次数）
- **奖励领取**：任务完成后自动领奖、大转盘抽奖、盲盒开箱
- **多账号**：一键遍历所有国内版账号批量执行

> 需要真实对话的任务（GLM-5.2 对话、夜猫子活动、公益捐款等）暂不支持自动完成。

## macOS 权限说明

切换账号需要写入 WorkBuddy 认证文件，macOS 要求授权「App 管理」或「完全磁盘访问」：

1. 首次切换报「无权限」时，点「打开系统设置」
2. 在 **App 管理** 里打开 workbuddy-switch 开关；若没有则去 **完全磁盘访问** 添加
3. 授权后重启本应用生效

## 致谢

感谢 [Linux.do](https://linux.do) 社区及原项目 [changexbc/workbuddy-switch](https://github.com/changexbc/workbuddy-switch)。

## 许可

[MIT](./LICENSE)
