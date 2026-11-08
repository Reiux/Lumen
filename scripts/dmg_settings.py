import os

SCRIPT_DIR = os.path.dirname(os.path.abspath(__file__))
REPO_ROOT = os.path.abspath(os.path.join(SCRIPT_DIR, '..'))

try:
    _app = defines.get('app')
except NameError:
    _app = None

application = _app or os.environ.get('DMG_APP_PATH') or os.path.join(REPO_ROOT, 'target/universal-apple-darwin/release/bundle/macos/Lumen.app')
appname = os.path.basename(application)
app_base = os.path.splitext(appname)[0]

# Disk image format
format = 'UDZO'
size = None

# Content to include
files = [application]
symlinks = {'Applications': '/Applications'}
hide_extensions = [appname]

# Volume icon & background image
icon = os.path.join(REPO_ROOT, 'crates/lumen-tauri/icons/icon.icns')
background = os.path.join(REPO_ROOT, 'crates/lumen-tauri/icons/background.png')

# Window geometry (540x380 matching background.png exactly)
window_rect = ((200, 120), (540, 380))
default_view = 'icon-view'
icon_size = 128
text_size = 12

# Icon positions centered vertically at y=190
icon_locations = {
    appname: (130, 190),
    app_base: (130, 190),
    'Applications': (410, 190),
}
