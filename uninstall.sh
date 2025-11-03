#!/bin/bash

# Flint Launcher Uninstaller
echo "Uninstalling Flint Launcher..."

# Remove binary
echo "Removing binary..."
sudo rm -f /usr/local/bin/flint_launcher

# Remove desktop entry
echo "Removing desktop entry..."
sudo rm -f /usr/share/applications/flint_launcher.desktop

echo "Flint Launcher uninstalled successfully!"
echo "Note: Config directory ~/.config/flint was preserved."