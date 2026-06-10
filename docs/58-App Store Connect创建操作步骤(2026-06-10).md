# App Store Connect 创建操作步骤

本文档说明如何在 App Store Connect 中创建 `SeekMind` 的 App 记录。  
目标是先把应用条目建起来，后续再逐步补齐截图、描述、隐私说明和发布信息。

参考入口：

- https://appstoreconnect.apple.com/
- https://developer.apple.com/programs/

> 说明：App Store Connect 的具体界面会随 Apple 调整而变化，下面按当前常见流程描述。

## 1. 进入 App Store Connect

1. 打开 `https://appstoreconnect.apple.com/`
2. 使用具备 App Store Connect 权限的 Apple Developer 账号登录
3. 进入首页后，确认左侧菜单可见 `My Apps`

如果第一次登录后没有看到完整菜单，先确认：

- 账号是否已加入 Apple Developer Program
- 当前账号是否有 App Manager 或 Admin 权限

## 2. 创建新 App

1. 在左侧点击 `My Apps`
2. 点击左上角的 `+`
3. 选择 `New App`

在弹出的创建窗口里，通常需要填写以下字段：

- Platform
- Name
- Primary language
- Bundle ID
- SKU

### 2.1 Platform

选择：

- `macOS`

### 2.2 Name

填写：

- `SeekMind`

建议：

- 名称尽量与桌面应用图标和安装包名称保持一致
- 不要在创建时临时写测试名，后续改名会影响元数据一致性

### 2.3 Primary language

建议选择：

- `English (U.S.)`

原因：

- App Store Connect 的主语言对后续元数据结构有影响
- 当前项目本身支持中英文，主语言先以英文为主更通用

### 2.4 Bundle ID

选择项目对应的稳定 Bundle ID。

建议：

- 直接选已经在 Apple Developer 里注册好的正式 Bundle ID
- 不要选测试 Bundle ID

### 2.5 SKU

填写稳定内部标识，例如：

- `seekmind-desktop`

建议：

- SKU 只要在你自己的组织里唯一即可
- 这个字段通常不展示给用户，但要保持长期稳定

4. 填完后点击 `Create`

## 3. 创建后立即检查的内容

创建成功后，先不要急着补全部信息，先确认以下内容：

- App 条目是否已出现在 `My Apps`
- Bundle ID 是否正确
- 名称是否正确
- 语言是否正确
- 平台是否是 `macOS`

如果这里填错了，尽量先改正，再继续往下填，否则后面截图、描述和版本元数据都要跟着重做。

## 4. 创建后要补的基础信息

进入 App 条目后，通常建议先补这些基础信息：

### 4.1 App Information

重点检查：

- Name
- Subtitle
- Category
- Content Rights

建议：

- Category 选 `Productivity` 或 `Utilities`
- Content Rights 如果没有第三方受限内容，一般按实际情况填写

### 4.2 Pricing and Availability

如果当前不打算收费，可以先确认：

- 价格是否为免费
- 可用国家/地区是否符合预期

### 4.3 App Privacy

先把隐私信息的框架准备好。

当前项目是本地优先应用，建议后续在正式填报前确认：

- 是否收集用户数据
- 是否上传日志
- 是否调用外部模型
- 是否会发送网络请求

## 5. 上传构建版本之前的准备

在上传构建前，建议先确认：

- macOS 包已经正确签名
- Bundle ID 与 App Store Connect 中一致
- 版本号和构建号已按规范递增
- 自动更新配置不会和 App Store 版本号策略冲突

如果你打算同时保留直装版和商店版，版本号策略要先统一好，避免后续混乱。

## 6. 推荐的填写顺序

建议按下面顺序操作：

1. 创建 App 记录
2. 先补 App Information
3. 再补 Pricing and Availability
4. 然后补 App Privacy
5. 最后再上传构建和截图

这样做的好处是：

- 先把条目建起来
- 再逐步补齐材料
- 不会因为一开始信息不全而卡住创建流程

## 7. 当前项目的建议填写值

结合当前 DocMind 的状态，建议先填写：

- Name: `SeekMind`
- Platform: `macOS`
- Primary language: `English (U.S.)`
- Bundle ID: 使用正式的 macOS Bundle ID
- SKU: `seekmind-desktop`
- Category: `Productivity`

## 8. 常见错误

- `Bundle ID` 和本地打包配置不一致
- `SKU` 复用旧项目标识
- `Primary language` 选错后又在后面到处补语言
- 先上传构建再补基础信息，导致流程反复切换
- 把测试地址写成正式支持页

## 9. 创建完成后的下一步

App 条目创建后，下一步建议继续做：

- 补齐截图
- 补齐描述和关键词
- 补齐隐私政策和支持页
- 确认自动更新策略与商店版策略分离

