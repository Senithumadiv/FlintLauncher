#!/bin/bash

# Flint Launcher Installer
echo "Installing Flint Launcher..."

# Build in release mode
echo "Building Flint Launcher..."
cargo build --release

if [ $? -ne 0 ]; then
    echo "Build failed! Please check for errors."
    exit 1
fi

# Create directories if they don't exist
echo "Creating directories..."
sudo mkdir -p /usr/local/bin
sudo mkdir -p /usr/share/applications

# Install binary
echo "Installing binary to /usr/local/bin/flint_launcher..."
sudo cp target/release/flint_launcher /usr/local/bin/flint_launcher

# Make it executable
sudo chmod +x /usr/local/bin/flint_launcher

# Create desktop entry
echo "Creating desktop entry..."
cat > /tmp/flint_launcher.desktop << EOF
[Desktop Entry]
Version=1.0
Type=Application
Name=Flint Launcher
Comment=A fast application launcher
Exec=flint_launcher
Icon=system-search
Categories=Utility;
Keywords=launcher;search;app;
Terminal=false
StartupWMClass=flint_launcher
EOF

sudo cp /tmp/flint_launcher.desktop /usr/share/applications/
rm /tmp/flint_launcher.desktop

# Create config directory with default theme
echo "Setting up configuration..."
mkdir -p ~/.config/flint

# Create default theme if it doesn't exist
if [ ! -f ~/.config/flint/theme.conf ]; then
    cat > ~/.config/flint/theme.conf << 'EOF'
# Flint Theme Configuration
# Use hex colors like #RRGGBB

# Main window colors
background=#1e1e1e
text_color=#d4d4d4
selection_bg=#0078d4
selection_text=#ffffff
border_color=#3c3c3c

# Font settings
font_size=16
font_family=System

# Border radius
border_radius=0
EOF
    echo "Default theme created at ~/.config/flint/theme.conf"
fi

echo ""
echo "🎉 Flint Launcher installed successfully!"
echo ""
echo "You can now:"
echo "  - Run from terminal: flint_launcher"
echo "  - Bind to a keyboard shortcut (Super+R, Alt+Space, etc.)"
echo "  - Customize theme: ~/.config/flint/theme.conf"
echo ""
echo "To update, simply run this installer again."