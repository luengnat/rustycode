#!/usr/bin/env python3
"""Display rustycode theme color swatches"""

themes = [
    ("oc-1", {
        "background": "#f8f7f7",
        "foreground": "#8e8b8b",
        "primary": "#dcde8d",
        "secondary": "#d0d0d0",
        "accent": "#034cff",
        "success": "#12c905",
        "warning": "#ffdc17",
        "error": "#fc533a",
    }),
    ("tokyo-night", {
        "background": "#1a1b26",
        "foreground": "#a9b1d6",
        "primary": "#7aa2f7",
        "secondary": "#414868",
        "accent": "#bb9af7",
        "success": "#9ece6a",
        "warning": "#e0af68",
        "error": "#f7768e",
    }),
    ("dracula", {
        "background": "#282a36",
        "foreground": "#f8f8f2",
        "primary": "#bd93f9",
        "secondary": "#44475a",
        "accent": "#ff79c6",
        "success": "#50fa7b",
        "warning": "#ffb86c",
        "error": "#ff5555",
    }),
    ("monokai", {
        "background": "#272822",
        "foreground": "#f8f8f2",
        "primary": "#a6e22e",
        "secondary": "#3e3d32",
        "accent": "#f92672",
        "success": "#a6e22e",
        "warning": "#e6db74",
        "error": "#f92672",
    }),
    ("nord", {
        "background": "#2e3440",
        "foreground": "#eceff4",
        "primary": "#88c0d0",
        "secondary": "#4c566a",
        "accent": "#81a1c1",
        "success": "#a3be8c",
        "warning": "#ebcb8b",
        "error": "#bf616a",
    }),
    ("gruvbox-dark", {
        "background": "#282828",
        "foreground": "#ebdbb2",
        "primary": "#83a598",
        "secondary": "#504945",
        "accent": "#d3869b",
        "success": "#b8bb26",
        "warning": "#fabd2f",
        "error": "#fb4934",
    }),
    ("catppuccin-mocha", {
        "background": "#1e1e2e",
        "foreground": "#cdd6f4",
        "primary": "#89b4fa",
        "secondary": "#45475a",
        "accent": "#cba6f7",
        "success": "#a6e3a1",
        "warning": "#f9e2af",
        "error": "#f38ba8",
    }),
    ("rose-pine", {
        "background": "#191724",
        "foreground": "#e0def4",
        "primary": "#c4a7e7",
        "secondary": "#26233a",
        "accent": "#9ccfd8",
        "success": "#ebbcba",
        "warning": "#f6c177",
        "error": "#eb6f92",
    }),
    ("github-dark", {
        "background": "#0d1117",
        "foreground": "#c9d1d9",
        "primary": "#58a6ff",
        "secondary": "#21262d",
        "accent": "#bc8cff",
        "success": "#3fb950",
        "warning": "#d29922",
        "error": "#f85149",
    }),
]

def print_color_block(color_hex, label, width=8):
    """Print a color block with ANSI escape codes"""
    # Convert hex to RGB
    r = int(color_hex[1:3], 16)
    g = int(color_hex[3:5], 16)
    b = int(color_hex[5:7], 16)

    # Determine if we should use white or black text
    brightness = (r * 299 + g * 587 + b * 114) / 1000
    text_color = "30" if brightness > 128 else "37"

    # Print color block
    print(f"\033[48;2;{r};{g};{b}m\033[{text_color}m{label:^{width}}\033[0m", end="")

def display_theme(name, colors):
    """Display a single theme with all colors"""
    print(f"\n{name}")
    print("─" * 60)

    # Print all 8 colors in a row
    print_color_block(colors["background"], "BG", 10)
    print_color_block(colors["foreground"], "FG", 10)
    print_color_block(colors["primary"], "Pri", 10)
    print_color_block(colors["secondary"], "Sec", 10)
    print_color_block(colors["accent"], "Acc", 10)
    print_color_block(colors["success"], "Ok", 10)
    print_color_block(colors["warning"], "Warn", 10)
    print_color_block(colors["error"], "Err", 10)
    print()

    # Show hex codes
    print(f"  BG:  {colors['background']}")
    print(f"  FG:  {colors['foreground']}")
    print(f"  Pri: {colors['primary']}")
    print(f"  Sec: {colors['secondary']}")
    print(f"  Acc: {colors['accent']}")
    print(f"  Ok:  {colors['success']}")
    print(f"  Warn:{colors['warning']}")
    print(f"  Err: {colors['error']}")

print("\n" + "="*60)
print("RUSTYCODE THEME PREVIEW")
print("="*60)

for theme_name, theme_colors in themes:
    display_theme(theme_name, theme_colors)

print("\n" + "="*60)
print("Use /theme [name] in TUI to switch themes")
print("="*60 + "\n")
