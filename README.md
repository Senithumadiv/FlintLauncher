# Flint Launcher ⚡

A fast, customizable application launcher written in Rust. Inspired by Rofi with Tokyo Night theming out of the box.

![Flint Launcher](https://img.shields.io/badge/rust-%23000000.svg?style=for-the-badge&logo=rust&logoColor=white)
![License](https://img.shields.io/badge/license-MIT-blue.svg)

## ✨ Features

- **Lightning Fast** - Instant app searching and launching
- **Fuzzy Search** - Find apps even with typos
- **Custom Themes** - Easy theming with simple config files
- **System Font Support** - Use any installed font on your system
- **Minimal UI** - Clean, distraction-free interface
- **Keyboard Driven** - Full keyboard navigation

## 🚀 Installation

### Prerequisites
- Rust and Cargo (for building from source)
- GTK (for app launching)

### Quick Install
```bash
# Clone the repository
git clone https://github.com/Senithumadiv/FlintLauncher.git
cd FlintLauncher

# Run the installer
chmod +x install.sh
./install.sh

### Manual Installation
```bash
git clone https://github.com/Senithumadiv/FlintLauncher.git
cd FlintLauncher
cargo build --release
sudo cp target/release/flint_launcher /usr/local/bin/
sudo chmod +x /usr/local/bin/flint_launcher
mkdir -p ~/.config/flint

## 🎯 Usage
### Run from Terminal
```bash
flint_launcher

## 🎨 Theming
Flint supports full customization via:


### Default Theme (Tokyo Night)
```conf
background=#1a1b26
text_color=#c0caf5
selection_bg=#7aa2f7
selection_text=#1a1b26
border_color=#414868
font_size=16
font_family=System
border_radius=0

### Everblush Theme
```conf
background=#141b1e
text_color=#dadada
selection_bg=#e57474
selection_text=#141b1e
border_color=#2b3339
font_size=16
font_family=JetBrains Mono
border_radius=6

### Light Theme
```conf
background=#ffffff
text_color=#2e3440
selection_bg=#5e81ac
selection_text=#ffffff
border_color=#d8dee9
font_size=16
font_family=System
border_radius=6

### Catppuccin Macchiato
```conf
background=#24273a
text_color=#cad3f5
selection_bg=#8aadf4
selection_text=#24273a
border_color=#494d64
font_size=16
font_family=Fira Code
border_radius=8

### Custom Fonts
```conf
font_family=ZedMono Nerd Font Mono
font_family=Fira Code
font_family=JetBrains Mono

## 🛠️ Building from Source
```bash
git clone https://github.com/Senithumadiv/FlintLauncher.git
cd FlintLauncher
cargo build --release

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

## 📝 License
Licensed under the MIT License

## 🤝 Contributing
Contributions welcome! You can:
- Report bugs  
- Suggest features  
- Share new themes  

If you like Flint Launcher, give it a ⭐ on GitHub!  
Happy launching! 🚀
