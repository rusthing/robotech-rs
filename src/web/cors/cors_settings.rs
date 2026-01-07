use serde::{Deserialize, Serialize};
use wheel_rs::serde::vec_option_serde;

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "kebab-case")]
pub struct CorsSettings {
    /// # 允许哪些来源（域名）
    /// ## 作用与原理
    /// - 控制浏览器允许哪些源(网站)的前端代码可以向此服务器发出请求
    /// - 浏览器会在请求中自动带上 Origin 请求头，并据此检查此请求是否在服务器预检时响应回的源列表中
    /// - 对应服务器预检时的响应头: Access-Control-Allow-Origin
    /// ## 注意事项
    /// - 源 = 协议 + 域名 + 端口，必须精确匹配，包括协议、域名、端口
    /// - http://localhost:3000 和 http://localhost:3001 是不同的源
    /// - 默认为*，不建议在生产环境使用 *（允许所有源），这会有安全风险
    /// ## 场景示例
    /// 前端地址: http://localhost:3000/>
    /// 后端地址: http://localhost:8080 <br/>
    /// 需要配置: http://localhost:3000 <br/>
    #[serde(with = "vec_option_serde", default = "allowed_origins_default")]
    pub allowed_origins: Option<Vec<String>>,
    /// # 允许哪些HTTP方法
    /// ## 作用与原理
    /// - 控制浏览器允许前端代码可以使用哪些HTTP方法向此服务器发出请求
    /// - 对应服务器预检时的响应头: Access-Control-Allow-Methods
    /// ## 注意事项
    /// - 默认为只允许简单请求方法：GET, POST, HEAD，其他方法需要显式添加
    /// - 生产环境中不建议使用通配符 *
    #[serde(with = "vec_option_serde", default = "allowed_methods_default")]
    pub allowed_methods: Option<Vec<String>>,
    /// # 允许哪些HTTP头
    /// ## 作用与原理
    /// - 控制浏览器允许前端代码可以携带哪些HTTP请求头向此服务器发出请求
    /// - 对应服务器预检时的响应头: Access-Control-Allow-Headers
    /// ## 注意事项
    /// - 对于"非简单请求头"，浏览器会先发预检请求检查这些头是否被允许
    /// - 简单请求头(不需要预检) <br/>
    /// -- Accept <br/>
    /// -- Accept-Language <br/>
    /// -- Content-Language <br/>
    /// -- Content-Type（仅限 application/x-www-form-urlencoded, multipart/form-data, text/plain
    #[serde(with = "vec_option_serde", default = "allowed_headers_default")]
    pub allowed_headers: Option<Vec<String>>,
    /// # 暴露哪些HTTP头
    /// ## 作用与原理
    /// - 控制浏览器允许前端代码可以访问哪些HTTP响应头
    /// - 对应服务器预检时的响应头: Access-Control-Expose-Headers
    /// - 默认情况下，前端只能读取基本的响应头（如 Content-Type），这个选项让前端能读取额外的自定义响应头
    /// ## 注意事项
    /// - 默认情况下，浏览器只允许 JS 读取 “安全列表” 中的响应头 https://fetch.spec.whatwg.org/#cors-safelisted-response-header-name <br/>
    /// -- Cache-Control <br/>
    /// -- Content-Language <br/>
    /// -- Content-Length <br/>
    /// -- Content-Type <br/>
    /// -- Expires <br/>
    /// -- Last-Modified <br/>
    /// -- Pragma <br/>
    /// -- 非禁止的响应头名称的任何项 https://fetch.spec.whatwg.org/#forbidden-response-header-name
    #[serde(with = "vec_option_serde", default = "expose_headers_default")]
    pub expose_headers: Option<Vec<String>>,
    /// # 预检请求的缓存时间(秒)
    /// ## 作用与原理
    /// - 浏览器会先发预检请求检查是否允许使用某些HTTP方法、HTTP头、HTTP响应头
    /// - 预检请求的缓存时间，浏览器会在该时间段内不再发送预检请求，而是直接使用缓存的预检结果
    /// - 对应服务器预检时的响应头: Access-Control-Max-Age
    /// ## 注意事项
    /// - 推荐值 <br/>
    /// -- 开发环境：0(方便调试) <br/>
    /// -- 生产环境：不设置(默认1800，即30分钟)、3600(1小时) 或 86400(24小时)
    #[serde(default = "max_age_default")]
    pub max_age: Option<usize>,
    /// # 是否允许携带凭证
    /// ## 作用与原理
    /// 控制浏览器允许前端代码可以携带哪些用户凭据，如:
    /// - Cookies(需前端 fetch 设置 credentials: 'include')
    /// - HTTP 认证信息
    /// - TLS 客户端证书
    /// 对应服务器预检时的响应头: Access-Control-Allow-Credentials
    /// ## 注意事项
    /// - 默认情况下，浏览器不允许携带凭证
    /// - 如果启用了 supports_credentials，就不能使用通配符 * 作为 allowed_origin
    /// ## 使用场景
    /// - 需要: 使用基于 Session/Cookie 的认证
    /// - 需要: 在跨域请求中读写 Cookie
    /// - 不需要: 使用无状态 JWT（存在 localStorage）
    #[serde(default = "supports_credentials_default")]
    pub supports_credentials: Option<bool>,
}

fn allowed_origins_default() -> Option<Vec<String>> {
    None
}
fn allowed_methods_default() -> Option<Vec<String>> {
    None
}
fn allowed_headers_default() -> Option<Vec<String>> {
    None
}
fn expose_headers_default() -> Option<Vec<String>> {
    None
}
fn max_age_default() -> Option<usize> {
    Some(1800)
}
fn supports_credentials_default() -> Option<bool> {
    None
}
