use crate::config::load_app_config;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{Duration, Instant};

const CACHE_TTL: Duration = Duration::from_secs(30 * 60);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(12);
const DEFAULT_BASE_URL: &str = "https://newsnow.busiyi.world";
// Cloudflare blocks the default Rust reqwest UA, so emulate a browser.
const BROWSER_USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) \
    AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0.0.0 Safari/537.36";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NewsSourceView {
    pub id: &'static str,
    pub name: &'static str,
    pub category: &'static str,
}

/// Curated, verified source list (each id was probed against the live API).
/// Users pick which ones to show inside the panel.
pub const NEWS_SOURCES: &[NewsSourceView] = &[
    NewsSourceView {
        id: "zhihu",
        name: "知乎",
        category: "综合热榜",
    },
    NewsSourceView {
        id: "weibo",
        name: "微博",
        category: "综合热榜",
    },
    NewsSourceView {
        id: "baidu",
        name: "百度",
        category: "综合热榜",
    },
    NewsSourceView {
        id: "toutiao",
        name: "今日头条",
        category: "综合热榜",
    },
    NewsSourceView {
        id: "douyin",
        name: "抖音",
        category: "综合热榜",
    },
    NewsSourceView {
        id: "bilibili",
        name: "哔哩哔哩",
        category: "综合热榜",
    },
    NewsSourceView {
        id: "ithome",
        name: "IT之家",
        category: "科技",
    },
    NewsSourceView {
        id: "sspai",
        name: "少数派",
        category: "科技",
    },
    NewsSourceView {
        id: "juejin",
        name: "掘金",
        category: "科技",
    },
    NewsSourceView {
        id: "solidot",
        name: "Solidot",
        category: "科技",
    },
    NewsSourceView {
        id: "coolapk",
        name: "酷安",
        category: "科技",
    },
    NewsSourceView {
        id: "v2ex",
        name: "V2EX",
        category: "社区",
    },
    NewsSourceView {
        id: "nowcoder",
        name: "牛客网",
        category: "社区",
    },
    NewsSourceView {
        id: "hackernews",
        name: "Hacker News",
        category: "国际",
    },
    NewsSourceView {
        id: "producthunt",
        name: "Product Hunt",
        category: "国际",
    },
    NewsSourceView {
        id: "github-trending-today",
        name: "GitHub Trending",
        category: "国际",
    },
    NewsSourceView {
        id: "jin10",
        name: "金十数据",
        category: "财经",
    },
    NewsSourceView {
        id: "wallstreetcn",
        name: "华尔街见闻",
        category: "财经",
    },
    NewsSourceView {
        id: "xueqiu",
        name: "雪球",
        category: "财经",
    },
    NewsSourceView {
        id: "zaobao",
        name: "联合早报",
        category: "其他",
    },
];

pub fn sources() -> &'static [NewsSourceView] {
    NEWS_SOURCES
}

pub fn base_url() -> String {
    load_app_config()
        .news_base_url
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.trim_end_matches('/').to_string())
        .unwrap_or_else(|| DEFAULT_BASE_URL.to_string())
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NewsItem {
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NewsResult {
    pub source: String,
    pub updated_time: Option<u64>,
    pub cached: bool,
    pub items: Vec<NewsItem>,
}

#[derive(Debug, Deserialize)]
struct RawNewsItem {
    #[serde(default)]
    id: serde_json::Value,
    #[serde(default)]
    title: String,
    #[serde(default)]
    url: Option<String>,
}

#[derive(Debug, Deserialize)]
struct NewsApiResponse {
    #[serde(default)]
    status: String,
    #[serde(default, rename = "updatedTime")]
    updated_time: u64,
    #[serde(default)]
    items: Vec<RawNewsItem>,
}

#[derive(Debug, Clone)]
struct CacheEntry {
    items: Vec<NewsItem>,
    updated_time: Option<u64>,
    fetched_at: Instant,
}

static CACHE: Mutex<Option<HashMap<String, CacheEntry>>> = Mutex::new(None);

pub async fn fetch_source(source: &str, force: bool) -> Result<NewsResult, String> {
    let trimmed = source.trim();
    if trimmed.is_empty() {
        return Err("数据源为空".to_string());
    }

    if !force {
        if let Some(fresh) = read_fresh_cache(trimmed) {
            return Ok(fresh);
        }
    }

    match fetch_remote(trimmed).await {
        Ok(result) => Ok(result),
        Err(error) => read_any_cache(trimmed).ok_or(error),
    }
}

fn read_fresh_cache(source: &str) -> Option<NewsResult> {
    let cache = CACHE.lock();
    let entry = cache.as_ref()?.get(source)?;
    if entry.fetched_at.elapsed() >= CACHE_TTL {
        return None;
    }
    Some(cached_result(source, entry))
}

fn read_any_cache(source: &str) -> Option<NewsResult> {
    let cache = CACHE.lock();
    let entry = cache.as_ref()?.get(source)?;
    Some(cached_result(source, entry))
}

fn cached_result(source: &str, entry: &CacheEntry) -> NewsResult {
    NewsResult {
        source: source.to_string(),
        updated_time: entry.updated_time,
        cached: true,
        items: entry.items.clone(),
    }
}

async fn fetch_remote(source: &str) -> Result<NewsResult, String> {
    let url = format!("{}/api/s?id={}", base_url(), source);
    let mut builder = reqwest::Client::builder()
        .timeout(REQUEST_TIMEOUT)
        .user_agent(BROWSER_USER_AGENT);
    let config = crate::config::load_app_config();
    if let Some(proxy_url) = config
        .proxy_url
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        if let Ok(proxy) = reqwest::Proxy::all(proxy_url) {
            builder = builder.proxy(proxy);
        }
    }
    let client = builder
        .build()
        .map_err(|error| format!("构建请求失败: {error}"))?;

    let response = client
        .get(&url)
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(|error| format!("网络请求失败: {error}"))?;

    if !response.status().is_success() {
        return Err(format!("数据源返回状态 {}", response.status()));
    }

    let api: NewsApiResponse = response
        .json()
        .await
        .map_err(|error| format!("解析响应失败: {error}"))?;

    if api.status == "error" {
        return Err("数据源暂时不可用(可能过载),可稍后点刷新重试".to_string());
    }

    let items: Vec<NewsItem> = api
        .items
        .into_iter()
        .filter_map(|raw| {
            let title = raw.title.trim().to_string();
            if title.is_empty() {
                return None;
            }
            let url = raw.url.or_else(|| {
                raw.id
                    .as_str()
                    .filter(|value| value.starts_with("http"))
                    .map(str::to_string)
            });
            Some(NewsItem { title, url })
        })
        .collect();

    let updated_time = if api.updated_time > 0 {
        Some(api.updated_time)
    } else {
        None
    };

    {
        let mut cache = CACHE.lock();
        let map = cache.get_or_insert_with(HashMap::new);
        map.insert(
            source.to_string(),
            CacheEntry {
                items: items.clone(),
                updated_time,
                fetched_at: Instant::now(),
            },
        );
    }

    Ok(NewsResult {
        source: source.to_string(),
        updated_time,
        cached: false,
        items,
    })
}
