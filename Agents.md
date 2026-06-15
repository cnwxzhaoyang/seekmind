- 每个代码文件头必须添加注释，至少包括Author:MorningSun ，CreatedDate,Description等
  例如对于java文件
  ```java
  /**
  * @author MorningSun
  * @CreatedDate 2026/5/19
  * @Description API认证持久化层
  */
  public interface ApiAuthorizationRepository {
      ApiAuthorization save(ApiAuthorization entity);
      ApiAuthorization findById(Long id);
      void deleteById(Long id);
      List<ApiAuthorization> findAll();
      ApiAuthorization findEnabled(Long appId, Long apiVersionId, String env);
      long countAll();
      List<ApiAuthorization> findPage(int offset, int limit);
      long countByAppIdIn(java.util.Collection<Long> appIds);
      List<ApiAuthorization> findPageByAppIdIn(java.util.Collection<Long> appIds, int offset, int limit);
  }
  ```
- 修复问题及bug时，必须留有注释
- 代码中关键的地方必须要打印日志
- 对于Java语言，定义Bean时，不要写大量的getter和setter，使用lombok注解
- 对于java语言，涉及到bean转换时，优先使用MapStruct
- 前端vue文件行数约束在2000行以内，如果超过2000行，就要考虑组件的拆分
- rust开发，单个rs文件行数不要超过2000
- 涉及文件路径、目录树、索引目录、文档筛选、数据库路径匹配、文件打开/预览等能力时，必须同时兼容 Windows 和 macOS：
  - 禁止只按单一分隔符处理路径，比较、前缀判断、父子目录判断前必须统一规范化路径
  - 新增或修改路径相关逻辑时，必须同时检查 `\` 和 `/` 两种情况，避免一端正常、另一端平铺/筛选为空/统计错误
  - 如涉及持久化到数据库，必须考虑历史数据兼容，避免仅因路径格式差异导致查询失效
  - 除非业务明确要求，否则不要把实现写死为某一平台专属路径格式
- `ui-prototype` 目录只作为设计参考，不参与前端编译链和运行时资源加载；图标、SVG、静态资源必须放入项目前端目录（例如 `src/assets`）并通过源码树引用，禁止在代码中直接依赖 `ui-prototype` 路径
- 执行java的编译时，编译环境优先使用项目路径下.sdkmanrc
- UI风格：
  - 必须支持国际化，至少应该支持中英文切换
  - 这是桌面端应用，原则要保持简洁，不要引入过多的线条
  - 减少按钮的使用，尽可能使用右击菜单栏的方式
  - 创建按钮时，按钮图标要贴近主题
  - 全局颜色、边框、阴影、输入框、下拉框、标签、toast、卡片层级必须优先复用统一主题 token，不要在页面里散写白底、浅蓝底、浅灰底
  - 深色主题下，页面背景、面板背景、卡片背景、选中态背景必须明显分层，卡片和外围背景不能混在一起
  - 选中态要统一口径，集合卡、条目卡、筛选 chip 等交互元素尽量共用同一套选中样式
  - 输入框、select、textarea、标签编辑器、按钮等基础控件要复用同一套深浅色样式，避免局部出现原生白底
  - 临时提示、错误提示、成功提示优先使用浮动 toast，不要嵌在主布局里
  - 日期展示默认只显示到年月日，除非业务明确需要时分秒
  - 如果要为Card、Grid添加标题，如果子标题的含义完全和主标题一样，就不用加子标题，如果加入子标题，尽量保持主副标题在同一行，不要上下堆叠
  - Grid、Tree等文字长度超出被截断后，应该保留光标移至显示tip
  - Grid,Tree等通用组件整个项目的样式应该保持同一，字体、字号、行间距等
