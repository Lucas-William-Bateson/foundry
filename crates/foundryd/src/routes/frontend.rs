use axum::{
    extract::{Path, State},
    response::Html,
    routing::get,
    Router,
};
use std::sync::Arc;

use crate::db;
use crate::AppState;

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/", get(index))
        .route("/job/{id}", get(job_detail))
}

async fn index(State(state): State<Arc<AppState>>) -> Html<String> {
    let jobs = db::list_jobs(&state.db, 50).await.unwrap_or_default();

    let mut rows = String::new();
    for job in jobs {
        let status_class = match job.status.as_str() {
            "success" => "status-success",
            "failed" => "status-failed",
            "running" => "status-running",
            _ => "status-queued",
        };
        let status_icon = match job.status.as_str() {
            "success" => "✅",
            "failed" => "❌",
            "running" => "🔄",
            _ => "⏳",
        };
        rows.push_str(&format!(
            r#"<tr>
                <td><a href="/job/{}">{}</a></td>
                <td>{}/{}</td>
                <td><code>{}</code></td>
                <td><span class="{}">{} {}</span></td>
                <td>{}</td>
            </tr>"#,
            job.id,
            job.id,
            job.repo_owner,
            job.repo_name,
            &job.git_sha[..8.min(job.git_sha.len())],
            status_class,
            status_icon,
            job.status,
            job.created_at,
        ));
    }

    let html = format!(
        r#"<!DOCTYPE html>
<html>
<head>
    <title>Foundry CI</title>
    <meta name="viewport" content="width=device-width, initial-scale=1">
    <style>
        :root {{
            --bg: #0d1117;
            --fg: #c9d1d9;
            --border: #30363d;
            --link: #58a6ff;
            --success: #3fb950;
            --failure: #f85149;
            --running: #d29922;
            --queued: #8b949e;
        }}
        * {{ box-sizing: border-box; }}
        body {{
            font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Helvetica, Arial, sans-serif;
            background: var(--bg);
            color: var(--fg);
            margin: 0;
            padding: 20px;
            line-height: 1.5;
        }}
        .container {{ max-width: 1200px; margin: 0 auto; }}
        h1 {{ 
            border-bottom: 1px solid var(--border);
            padding-bottom: 16px;
            font-size: 24px;
        }}
        table {{
            width: 100%;
            border-collapse: collapse;
            margin-top: 16px;
        }}
        th, td {{
            padding: 12px;
            text-align: left;
            border-bottom: 1px solid var(--border);
        }}
        th {{ font-weight: 600; }}
        a {{ color: var(--link); text-decoration: none; }}
        a:hover {{ text-decoration: underline; }}
        code {{
            background: #161b22;
            padding: 2px 6px;
            border-radius: 4px;
            font-size: 13px;
        }}
        .status-success {{ color: var(--success); }}
        .status-failed {{ color: var(--failure); }}
        .status-running {{ color: var(--running); }}
        .status-queued {{ color: var(--queued); }}
        .empty {{ 
            text-align: center; 
            padding: 40px;
            color: var(--queued);
        }}
    </style>
</head>
<body>
    <div class="container">
        <h1>🏭 Foundry CI</h1>
        <table>
            <thead>
                <tr>
                    <th>Job</th>
                    <th>Repository</th>
                    <th>Commit</th>
                    <th>Status</th>
                    <th>Created</th>
                </tr>
            </thead>
            <tbody>
                {rows}
            </tbody>
        </table>
        {empty}
    </div>
</body>
</html>"#,
        rows = rows,
        empty = if rows.is_empty() {
            r#"<p class="empty">No jobs yet. Push a commit to get started!</p>"#
        } else {
            ""
        }
    );

    Html(html)
}

async fn job_detail(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> Html<String> {
    let job = match db::get_job(&state.db, id).await {
        Ok(Some(job)) => job,
        Ok(None) => return Html("<h1>Job not found</h1>".to_string()),
        Err(e) => return Html(format!("<h1>Error: {}</h1>", e)),
    };

    let logs = db::get_job_logs(&state.db, id)
        .await
        .unwrap_or_default()
        .unwrap_or_else(|| "No logs available".to_string());

    let status_class = match job.status.as_str() {
        "success" => "status-success",
        "failed" => "status-failed",
        "running" => "status-running",
        _ => "status-queued",
    };
    let status_icon = match job.status.as_str() {
        "success" => "✅",
        "failed" => "❌",
        "running" => "🔄",
        _ => "⏳",
    };

    let html = format!(
        r#"<!DOCTYPE html>
<html>
<head>
    <title>Job #{} - Foundry CI</title>
    <meta name="viewport" content="width=device-width, initial-scale=1">
    <style>
        :root {{
            --bg: #0d1117;
            --fg: #c9d1d9;
            --border: #30363d;
            --link: #58a6ff;
            --success: #3fb950;
            --failure: #f85149;
            --running: #d29922;
            --queued: #8b949e;
        }}
        * {{ box-sizing: border-box; }}
        body {{
            font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Helvetica, Arial, sans-serif;
            background: var(--bg);
            color: var(--fg);
            margin: 0;
            padding: 20px;
            line-height: 1.5;
        }}
        .container {{ max-width: 1200px; margin: 0 auto; }}
        h1 {{ 
            font-size: 24px;
            margin-bottom: 8px;
        }}
        .back {{ margin-bottom: 16px; display: block; }}
        a {{ color: var(--link); text-decoration: none; }}
        a:hover {{ text-decoration: underline; }}
        .meta {{
            background: #161b22;
            border: 1px solid var(--border);
            border-radius: 8px;
            padding: 16px;
            margin-bottom: 20px;
        }}
        .meta-row {{
            display: flex;
            margin-bottom: 8px;
        }}
        .meta-row:last-child {{ margin-bottom: 0; }}
        .meta-label {{
            font-weight: 600;
            width: 100px;
            flex-shrink: 0;
        }}
        code {{
            background: #161b22;
            padding: 2px 6px;
            border-radius: 4px;
            font-size: 13px;
        }}
        .status-success {{ color: var(--success); }}
        .status-failed {{ color: var(--failure); }}
        .status-running {{ color: var(--running); }}
        .status-queued {{ color: var(--queued); }}
        .logs {{
            background: #161b22;
            border: 1px solid var(--border);
            border-radius: 8px;
            padding: 16px;
            font-family: 'SFMono-Regular', Consolas, 'Liberation Mono', Menlo, monospace;
            font-size: 12px;
            white-space: pre-wrap;
            word-break: break-all;
            overflow-x: auto;
            max-height: 600px;
            overflow-y: auto;
        }}
        h2 {{
            font-size: 18px;
            margin-top: 24px;
            margin-bottom: 12px;
        }}
    </style>
</head>
<body>
    <div class="container">
        <a href="/" class="back">← Back to jobs</a>
        <h1>Job #{id}</h1>
        
        <div class="meta">
            <div class="meta-row">
                <span class="meta-label">Repository</span>
                <span>{owner}/{name}</span>
            </div>
            <div class="meta-row">
                <span class="meta-label">Commit</span>
                <code>{sha}</code>
            </div>
            <div class="meta-row">
                <span class="meta-label">Ref</span>
                <span>{git_ref}</span>
            </div>
            <div class="meta-row">
                <span class="meta-label">Status</span>
                <span class="{status_class}">{status_icon} {status}</span>
            </div>
            <div class="meta-row">
                <span class="meta-label">Created</span>
                <span>{created}</span>
            </div>
        </div>

        <h2>Build Logs</h2>
        <div class="logs">{logs}</div>
    </div>
</body>
</html>"#,
        id = job.id,
        owner = job.repo_owner,
        name = job.repo_name,
        sha = job.git_sha,
        git_ref = job.git_ref,
        status_class = status_class,
        status_icon = status_icon,
        status = job.status,
        created = job.created_at,
        logs = html_escape(&logs),
    );

    Html(html)
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
