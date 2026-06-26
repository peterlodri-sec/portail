import marimo

__generated_with = "0.1.0"
app = marimo.App()


@app.cell
def __():
    import marimo as mo
    import httpx
    import json
    from datetime import datetime
    return datetime, httpx, json, mo


@app.cell
def __(mo):
    mo.md(
        """
        # Portail Network Visualization
        
        **Live dashboard** for portail proxy/gateway network activity.
        
        - **Source**: [github.com/peterlodri-sec/portail](https://github.com/peterlodri-sec/portail)
        - **Docs**: [portail.vaked.dev](https://portail.vaked.dev)
        """
    )
    return


@app.cell
def __(mo):
    # Configuration
    portail_url = mo.ui.text(
        value="http://localhost:8787",
        label="Portail URL"
    )
    portail_url
    return (portail_url,)


@app.cell
def __(httpx, portail_url):
    # Fetch CI status
    try:
        resp = httpx.get(f"{portail_url.value}/ci/status", timeout=5)
        ci_status = resp.json()
    except Exception as e:
        ci_status = {"overall": "offline", "error": str(e)}
    return (ci_status,)


@app.cell
def __(ci_status, mo):
    # Display CI status
    overall = ci_status.get("overall", "unknown")
    color_map = {
        "passing": "green",
        "failing": "red",
        "in_progress": "yellow",
        "unknown": "gray",
        "offline": "gray"
    }
    color = color_map.get(overall, "gray")
    
    mo.md(f"""
    ## CI Status
    
    <span style="color: {color}; font-size: 1.5em;">●</span> **{overall.upper()}**
    
    - Total runs: {ci_status.get("total_runs", 0)}
    - Success rate: {ci_status.get("success_rate", 0):.1%}
    """)
    return


@app.cell
def __(httpx, portail_url):
    # Fetch recent events
    try:
        resp = httpx.get(f"{portail_url.value}/events?n=20", timeout=5)
        events = resp.json()
    except Exception as e:
        events = []
    return (events,)


@app.cell
def __(events, mo):
    # Display events
    if events:
        event_lines = []
        for e in events[:10]:
            ts = e.get("timestamp", "")
            agent = e.get("agent_id", "unknown")
            etype = e.get("event_type", "unknown")
            event_lines.append(f"| {ts} | `{agent}` | `{etype}` |")
        
        event_table = "\n".join(event_lines)
        mo.md(f"""
        ## Recent Events
        
        | Timestamp | Agent | Type |
        |-----------|-------|------|
        {event_table}
        """)
    else:
        mo.md("## Recent Events\n\nNo events (portail not running)")
    return


@app.cell
def __(httpx, portail_url):
    # Fetch stats
    try:
        resp = httpx.get(f"{portail_url.value}/stats", timeout=5)
        stats = resp.json()
    except Exception as e:
        stats = {"error": str(e)}
    return (stats,)


@app.cell
def __(mo, stats):
    # Display stats
    if "error" not in stats:
        cdn = stats.get("cdn", {})
        mo.md(f"""
        ## System Stats
        
        - **Version**: {stats.get("version", "unknown")}
        - **CDN Entries**: {cdn.get("entries", 0)}
        - **CDN Hits**: {cdn.get("hits", 0)}
        - **CDN Misses**: {cdn.get("misses", 0)}
        """)
    else:
        mo.md(f"## System Stats\n\nError: {stats['error']}")
    return


@app.cell
def __(mo):
    mo.md("""
    ## Architecture
    
    ```
    Client → Portail → AI Provider
              │
              ├─ Cache (Moka + Redis)
              ├─ Hooks (prompt injection)
              ├─ Events (ring buffer + SSE)
              ├─ A2A (agent-to-agent)
              ├─ A2C (agent-to-consumer)
              └─ Tracer (E2E tracing)
    ```
    """)
    return


@app.cell
def __(mo):
    # Auto-refresh toggle
    auto_refresh = mo.ui.checkbox(value=True, label="Auto-refresh (30s)")
    auto_refresh
    return (auto_refresh,)


if __name__ == "__main__":
    app.run()
