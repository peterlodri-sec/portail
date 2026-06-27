# portail/config.nu — project-level nushell config overrides
# This is sourced after the global config.nu

# ── Default editor ─────────────────────────────────────────────
$env.EDITOR = "hx"
$env.VISUAL = "hx"

# ── Color theme ────────────────────────────────────────────────
# Portail dev: use a clean, high-contrast theme
$env.config = {
    color_config: {
        separator: white
        leading_trailing_space_bg: {attr: n}
        header: green_bold
        date: yellow
    }
    edit_mode: vi
    history: {
        max_size: 10000
        file_format: "sqlite"
    }
    table: {
        mode: compact
        index_mode: auto
    }
}
