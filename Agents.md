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
- 执行java的编译时，编译环境优先使用项目路径下.sdkmanrc
- UI风格：
  - 必须支持国际化，至少应该支持中英文切换
  - 这是桌面端应用，原则要保持简洁，不要引入过多的线条
  - 减少按钮的使用，尽可能使用右击菜单栏的方式
  - 创建按钮时，按钮图标要贴近主题
  - 如果要为Card、Grid添加标题，如果子标题的含义完全和主标题一样，就不用加子标题，如果加入子标题，尽量保持主副标题在同一行，不要上下堆叠
  - Grid、Tree等文字长度超出被截断后，应该保留光标移至显示tip
  - Grid,Tree等通用组件整个项目的样式应该保持同一，字体、字号、行间距等
