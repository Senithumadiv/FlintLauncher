```ascii
    ███████╗ ██╗     ██╗███╗   ██╗████████╗
    ██╔════╝ ██║     ██║████╗  ██║╚══██╔══╝
    █████╗   ██║     ██║██╔██╗ ██║   ██║   
    ██╔══╝   ██║     ██║██║╚██╗██║   ██║   
    ██║      ███████╗██║██║ ╚████║   ██║   
    ╚═╝      ╚══════╝╚═╝╚═╝  ╚═══╝   ╚═╝   
    
          ⚡ L A U N C H E R ⚡
```

- A fast, customizable application launcher written in Rust for Linux with theming support.

![Flint Launcher](https://img.shields.io/badge/rust-%23000000.svg?style=for-the-badge&logo=rust&logoColor=white)

 ![License](https://img.shields.io/badge/license-MIT-blue.svg)

## ✨ Features

- **Lightning Fast** – Instant app searching and launching  
- **Fuzzy Search** – Find apps even with typos  
- **Custom Themes** – Easily configurable colors, borders, and fonts via `theme.conf`  
- **System Font Support** – Automatically detects installed fonts from common directories  
- **Highlight Matching** – Highlights matched characters in results  
- **Configurable Font Size & Radius** – Adjust UI scale and roundness to your preference  
- **Minimal UI** – Clean, distraction-free interface with centered layout  
- **Keyboard Driven** – Full keyboard navigation for quick control  
- **Multiple Search Modes** – Apps, URLs, web search, calculator, and commands
- **URL Detection** – Automatically detects and opens websites (youtube.com, github.com, etc.)
- **Smart Calculator** – Quick calculations
- **Shell Commands** – Execute terminal commands with `$` prefix
- **Web Search** – Search DuckDuckGo with `@` prefix
- **File Search** – Search files with `file:`
- **Emoji Search** – Search emoji with `e:`
- **Smooth Animations** – Beautiful fade-in and drop-down animations
- **Parallel Search** – Uses Rayon for smooth, non-blocking filtering
- **Instant Search**: Start typing after `:` to see matching emojis
- **Currency Exchange** Convert between 160+ currencies with live exchange rates.
- **App Icons** Display icons for apps using icon themes (configurable)

## 🚀 Installation

### Prerequisites
- Rust and Cargo (for building from source)
- GTK (for app launching)

### Quick Install

# Clone the repository
```bash
git clone https://github.com/Senithumadiv/FlintLauncher.git
cd FlintLauncher
```
### Run the installer
```bash
chmod +x install.sh
./install.sh
```
### Manual Installation
```bash
git clone https://github.com/Senithumadiv/FlintLauncher.git
cd FlintLauncher
cargo build --release
sudo cp target/release/flint_launcher /usr/local/bin/
sudo chmod +x /usr/local/bin/flint_launcher
mkdir -p ~/.config/flint
```

## 🎯 Usage
### Run from Terminal
```bash
flint_launcher
```

## 🎨 Theming
Flint supports full customization via:
```conf
~/.config/flint/theme.conf
```

### Font Setup

This app uses the **font file name**, not the internal font name.

To use a custom font, place your `.ttf` or `.otf` file in one of these folders:
```conf
~/.fonts
~/.local/share/fonts
/usr/share/fonts
```

Then set the font in the config using the **file name** (without the extension).  
For example, if your file is:

~/.local/share/fonts/JetBrainsMonoNerdFont-Regular.ttf

Use:
```bash
"font_family=JetBrainsMonoNerdFont-Regular"
```

If the app still says “Font not found”, make sure the file is readable and rebuild the font cache:

```conf
fc-cache -fv
```

### Everblush Theme
```conf
background=#141b1e
text_color=#dadada
selection_bg=#e57474
selection_text=#141b1e
border_color=#2b3339
highlight_color=#ffcc00
font_size=16
font_family=JetBrainsMonoNerdFont-Regular
```
### Light Theme
```conf
background=#ffffff
text_color=#2e3440
selection_bg=#5e81ac
selection_text=#ffffff
border_color=#d8dee9
highlight_color=#bf616a
font_size=16
font_family=JetBrainsMonoNerdFont-Regular
```

### Icons
You can customize app icons in `theme.conf`:

```conf
enable_icons=true
icon_theme=Papirus
icon_size=24
```

| Setting | Description | Default |
|---------|-------------|---------|
| `enable_icons` | Enable/disable app icons | `true` |
| `icon_size` | Size of icons in pixels | `24` |

Flint automatically detects your icon theme from:
- GNOME Settings (gsettings)
- XFCE Settings (xfconf)
- GTK3/GTK4 settings (~/.config/gtk-3.0/settings.ini, ~/.config/gtk-4.0/settings.ini)
- KDE Plasma (~/.config/kdeglobals)
- X Resources (~/.Xresources, ~/.Xsettingsd)
- LXAppearance

Place custom icon packs in `~/.icons/` or `/usr/share/icons/`.

### Catppuccin Macchiato
```conf
background=#24273a
text_color=#cad3f5
selection_bg=#8aadf4
selection_text=#24273a
border_color=#494d64
highlight_color=#f5a97f
font_size=16
font_family=JetBrainsMonoNerdFont-Regular
```

## 🛠️ Building from Source
```bash
git clone https://github.com/Senithumadiv/FlintLauncher.git
cd FlintLauncher
cargo build --release
```
Binary:  
`target/release/flint_launcher`

## ❓ Troubleshooting

**Launcher doesn’t appear?**
- Ensure a window manager is running  
- Check for conflicting shortcuts  

**Apps not showing?**
- Flint scans `.desktop` files in standard directories  
- Run `flint_launcher` in terminal for logs  

**Theme not applying?**
- Restart Flint after edits  
- Verify hex values and syntax

## 🗑️ Uninstallation
```bash
cd FlintLauncher
chmod +x uninstall.sh
./uninstall.sh
```

## 📝 License
Licensed under the MIT License

## 🤝 Contributing
Contributions welcome! You can:
- Report bugs  
- Suggest features  
- Share new themes  

If you like Flint Launcher, give it a ⭐ on GitHub!  