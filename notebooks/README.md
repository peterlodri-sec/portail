# Portail Notebooks

Interactive notebooks for exploring portail's capabilities.

## Marimo Notebook

[Marimo](https://marimo.io) is a reactive Python notebook that's perfect for live dashboards.

### Setup

```bash
# Install marimo
pip install marimo httpx

# Run the notebook
marimo edit notebooks/portail_viz.py
```

### Features

- **Live CI status** — Fetches from `/ci/status` endpoint
- **Event stream** — Shows recent events from `/events`
- **System stats** — Displays cache hits, entries, version
- **Auto-refresh** — Updates every 30 seconds
- **Architecture diagram** — Visual overview of portail

### Usage

1. Start portail: `portail serve`
2. Open notebook: `marimo edit notebooks/portail_viz.py`
3. Enter portail URL (default: `http://localhost:8787`)
4. View live data

## Notebooks

| Notebook | Description |
|----------|-------------|
| `portail_viz.py` | Live network visualization dashboard |

## Related

- [uwdata/visualization-curriculum](https://github.com/uwdata/visualization-curriculum) — Visualization curriculum
- [Marimo docs](https://docs.marimo.io) — Marimo documentation
