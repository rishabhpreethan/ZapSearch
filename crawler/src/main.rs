use anyhow::{anyhow, Result};
use clap::Parser;
use parking_lot::RwLock;
use reqwest::{header, Client, Url};
use scraper::{Html, Selector};
use serde::Serialize;
use std::collections::{HashMap, HashSet, VecDeque};
use std::fs::{self, File};
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::time::sleep;
use time::format_description::well_known::Rfc3339;
use sha1::{Sha1, Digest};

#[derive(Parser, Debug)]
#[command(name = "crawler")]
#[command(about = "Crawl the web to JSONL, respecting robots.txt")]
struct Cli {
    /// Path to a file with seed URLs (one per line)
    #[arg(long)]
    seeds: String,
    /// Output JSONL file path
    #[arg(long, default_value = "./sample_data/crawl.jsonl")]
    output: String,
    /// Maximum number of documents to fetch
    #[arg(long, default_value_t = 100_000)]
    max_docs: usize,
    /// Maximum pages to crawl per host (politeness)
    #[arg(long, default_value_t = 10)]
    max_per_host: usize,
    /// Concurrency (number of workers)
    #[arg(long, default_value_t = 16)]
    concurrency: usize,
    /// Request timeout seconds
    #[arg(long, default_value_t = 12)]
    timeout_secs: u64,
    /// User-Agent string to use for robots.txt and crawling
    #[arg(long, default_value = "search-engine-rs-bot/0.1 (+https://example.com/bot)")]
    user_agent: String,
    /// If true, only follow links that remain on the same host as the page
    #[arg(long, default_value_t = true)]
    same_host_only: bool,
}

#[derive(Debug, Clone)]
struct Robots {
    fetched_at: Instant,
    allows: Vec<String>,
    disallows: Vec<String>,
    crawl_delay_ms: Option<u64>,
}

#[derive(Default)]
struct Seen { urls: HashSet<String>, per_host: HashMap<String, usize> }

#[derive(Serialize)]
struct OutDoc<'a> {
    id: String,
    title: &'a str,
    body: &'a str,
    url: &'a str,
    timestamp: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Cli::parse();
    if let Some(dir) = std::path::Path::new(&args.output).parent() {
        fs::create_dir_all(dir).ok();
    }

    let client = Client::builder()
        .user_agent(args.user_agent.clone())
        .redirect(reqwest::redirect::Policy::limited(5))
        .timeout(Duration::from_secs(args.timeout_secs))
        .build()?;

    // Load seeds
    let mut frontier: VecDeque<Url> = VecDeque::new();
    for line in BufReader::new(File::open(&args.seeds)?).lines() {
        let s = line?.trim().to_string();
        if s.is_empty() || s.starts_with('#') { continue; }
        let u = Url::parse(&s).or_else(|_| Url::parse(&format!("https://{}", s)));
        if let Ok(u) = u { frontier.push_back(u); }
    }
    if frontier.is_empty() { return Err(anyhow!("no valid seeds")); }
    eprintln!(
        "crawler: seeds_loaded={} max_docs={} concurrency={} same_host_only={} max_per_host={} output={}",
        frontier.len(), args.max_docs, args.concurrency, args.same_host_only, args.max_per_host, args.output
    );

    let mut out = BufWriter::new(File::create(&args.output)?);
    let robots_cache: Arc<RwLock<HashMap<String, Robots>>> = Arc::new(RwLock::new(HashMap::new()));
    let mut seen = Seen::default();

    let sel_title = Selector::parse("title").unwrap();
    let sel_body = Selector::parse("body").unwrap();
    let sel_a = Selector::parse("a").unwrap();

    let mut emitted = 0usize;
    let mut inflight: Vec<tokio::task::JoinHandle<(Option<(String,String,String)>, Vec<Url>)>> = Vec::new();

    while emitted < args.max_docs && (!frontier.is_empty() || !inflight.is_empty()) {
        // Fill workers
        while inflight.len() < args.concurrency && !frontier.is_empty() && emitted + inflight.len() < args.max_docs {
            let url = frontier.pop_front().unwrap();
            let url_key = norm(&url);
            if seen.urls.contains(&url_key) { continue; }
            seen.urls.insert(url_key.clone());
            if let Some(h) = url.host_str() {
                let cnt = *seen.per_host.get(h).unwrap_or(&0);
                if cnt >= args.max_per_host { continue; }
                *seen.per_host.entry(h.to_string()).or_insert(0) = cnt + 1;
            }

            let client_c = client.clone();
            let robots_c = robots_cache.clone();
            let ua = args.user_agent.clone();
            let tsel = sel_title.clone();
            let bsel = sel_body.clone();
            let asel = sel_a.clone();

            let handle = tokio::spawn(async move {
                if !allowed(&client_c, &robots_c, &url, &ua).await.unwrap_or(false) {
                    return (None, vec![]);
                }
                if let Some(delay) = robots_delay(&robots_c, &url) { sleep(Duration::from_millis(delay)).await; }

                match client_c.get(url.clone()).send().await {
                    Ok(resp) => {
                        if !resp.status().is_success() { return (None, vec![]); }
                        if let Some(ct) = resp.headers().get(header::CONTENT_TYPE) {
                            if let Ok(v) = ct.to_str() { if !v.starts_with("text/html") { return (None, vec![]); } }
                        }
                        let bytes = match resp.bytes().await { Ok(b)=>b, Err(_)=>return (None, vec![]) };
                        if bytes.len() > 2*1024*1024 { return (None, vec![]); }
                        let body = String::from_utf8_lossy(&bytes).to_string();

                        let doc = Html::parse_document(&body);
                        let title = doc.select(&tsel).next().map(|n| n.text().collect::<String>()).unwrap_or_default();
                        let text = doc.select(&bsel).next().map(|n| n.text().collect::<String>()).unwrap_or_default();

                        let mut links = Vec::new();
                        for a in doc.select(&asel) {
                            if let Some(h) = a.value().attr("href") {
                                if let Ok(u) = Url::parse(h).or_else(|_| url.join(h)) {
                                    if u.scheme().starts_with("http") { links.push(u); }
                                }
                            }
                        }
                        (Some((norm(&url), title.trim().to_string(), text.trim().to_string())), links)
                    }
                    Err(_) => (None, vec![])
                }
            });
            inflight.push(handle);
        }

        if inflight.is_empty() { break; }

        let mut i = 0;
        while i < inflight.len() {
            if inflight[i].is_finished() {
                let h = inflight.swap_remove(i);
                if let Ok((doc, links)) = h.await {
                    for l in links {
                        if args.same_host_only {
                            if l.host_str() != doc.as_ref().and_then(|(u,_,_)| Url::parse(u).ok()).as_ref().and_then(|uu| uu.host_str()) { continue; }
                        }
                        frontier.push_back(l);
                    }
                    if let Some((u, t, b)) = doc {
                        let mut hasher = Sha1::new();
                        hasher.update(u.as_bytes());
                        let id = format!("{:x}", hasher.finalize());
                        let ts = time::OffsetDateTime::now_utc().format(&Rfc3339).unwrap_or_default();
                        let rec = OutDoc { id, title: &t, body: &b, url: &u, timestamp: ts };
                        serde_json::to_writer(&mut out, &rec).ok();
                        out.write_all(b"\n").ok();
                        emitted += 1;
                        if emitted % 100 == 0 {
                            eprintln!(
                                "progress: emitted={} visited={} frontier={}",
                                emitted,
                                seen.urls.len(),
                                frontier.len()
                            );
                        }
                    }
                }
            } else {
                i += 1;
            }
        }
    }

    eprintln!(
        "done: emitted={} visited={} frontier={} -> {}",
        emitted,
        seen.urls.len(),
        frontier.len(),
        &args.output
    );
    Ok(())
}

fn norm(u: &Url) -> String { let mut s = u.clone(); s.set_fragment(None); s.to_string() }

fn parse_robots(txt: &str) -> Robots {
    // minimal parser for the '*' group
    let mut active = false;
    let mut allows = Vec::new();
    let mut disallows = Vec::new();
    let mut crawl_delay_ms: Option<u64> = None;
    for line in txt.lines() {
        let l = line.trim();
        if l.is_empty() || l.starts_with('#') { continue; }
        if let Some((k, v)) = l.split_once(':') {
            let key = k.trim().to_lowercase();
            let val = v.trim();
            match key.as_str() {
                "user-agent" => { active = val == "*"; }
                "allow" if active => allows.push(val.to_string()),
                "disallow" if active => disallows.push(val.to_string()),
                "crawl-delay" if active => {
                    if let Ok(n) = val.parse::<f64>() { crawl_delay_ms = Some((n * 1000.0) as u64); }
                }
                _ => {}
            }
        }
    }
    Robots { fetched_at: Instant::now(), allows, disallows, crawl_delay_ms }
}

async fn allowed(client: &Client, cache: &Arc<RwLock<HashMap<String, Robots>>>, url: &Url, ua: &str) -> Result<bool> {
    let host = match url.host_str() { Some(h) => h.to_string(), None => return Ok(false) };
    let rules_opt = { let c = cache.read(); c.get(&host).cloned() };
    let rules = if let Some(r) = rules_opt { r } else {
        let robots_url = format!("{}://{}/robots.txt", url.scheme(), host);
        let txt = match client
            .get(&robots_url)
            .header(header::USER_AGENT, ua)
            .send()
            .await
        {
            Ok(resp) if resp.status().is_success() => resp.text().await.unwrap_or_default(),
            _ => String::new(),
        };
        let parsed = parse_robots(&txt);
        { let mut c = cache.write(); c.insert(host.clone(), parsed.clone()); }
        parsed
    };
    Ok(path_allowed(url.path(), &rules))
}

fn robots_delay(cache: &Arc<RwLock<HashMap<String, Robots>>>, url: &Url) -> Option<u64> {
    let host = url.host_str()?;
    cache.read().get(host).and_then(|r| r.crawl_delay_ms)
}

fn path_allowed(path: &str, rules: &Robots) -> bool {
    // basic rule precedence: longest matching Allow vs Disallow
    let mut best_allow: Option<&str> = None;
    let mut best_dis: Option<&str> = None;
    for a in &rules.allows { if path.starts_with(a) { if best_allow.map_or(true, |p| a.len() > p.len()) { best_allow = Some(a); } } }
    for d in &rules.disallows { if d == "/" { best_dis = Some(d); continue; } if path.starts_with(d) { if best_dis.map_or(true, |p| d.len() > p.len()) { best_dis = Some(d); } } }
    match (best_allow, best_dis) {
        (Some(a), Some(d)) => a.len() >= d.len(),
        (Some(_), None) => true,
        (None, Some(_)) => false,
        (None, None) => true,
    }
}
