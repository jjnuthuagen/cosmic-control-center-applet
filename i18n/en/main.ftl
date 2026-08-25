# Control Center — English strings.
#
# Every user-facing string in the applet lives here. Keep ids kebab-case and
# grouped by the tile or page they belong to.

applet-name = Control Center
applet-tooltip = Quick settings

# -- Shared actions -----------------------------------------------------------
back = Back
apply = Apply
connect = Connect
cancel = Cancel
connected = Connected
connecting = Connecting…
paired = Paired

# -- Connectivity (Wi-Fi + Bluetooth + VPN grouped tile) ----------------------
connectivity = Connectivity

# -- Wi-Fi --------------------------------------------------------------------
wifi = Wi-Fi
wifi-off = Off
wifi-disconnected = Not connected
wifi-connected = Connected
wifi-airplane = Airplane mode
wifi-hardware-off = Blocked by hardware switch
airplane-mode = Airplane mode
visible-networks = Visible networks
no-networks = No networks found
signal-strength = { $percent }%
enter-password = Password
enter-password-for = Enter the password for { $ssid }.
show-more = Show { $count } more
enterprise-in-settings = Set up in Settings
wifi-auth-failed = Could not join { $ssid } — check the password
wifi-needs-authorisation = Joining this network needs an administrator password.
wifi-timeout = { $ssid } did not respond
wifi-failed = Could not connect: { $reason }
ipv4 = IPv4
ipv6 = IPv6
mac = MAC

# -- Bluetooth ----------------------------------------------------------------
bluetooth = Bluetooth
bluetooth-off = Off
bluetooth-no-devices = No devices
bluetooth-devices = { $count ->
    [one] { $count } device
   *[other] { $count } devices
}
no-devices = No devices found
pair-in-settings = Pair in Settings

# -- Battery and power profiles -----------------------------------------------
battery = Battery
battery-charge = { $percent }%
battery-charging = { $percent }% charging
battery-no-battery = On mains
battery-remaining = { $time } remaining
battery-until-full = { $time } until full
power-profile = Power profile
profile-power-saver = Power Saver
profile-balanced = Balanced
profile-performance = Performance
performance-degraded = Performance limited: { $reason }
game-mode = Game Mode
game-mode-detail = Tunes the system for smoother gaming.
game-mode-held = A game is using this right now.

# -- DNS ----------------------------------------------------------------------
dns = DNS
dns-automatic = Automatic
dns-cloudflare = Cloudflare
dns-google = Google
dns-quad9 = Quad9
dns-custom = Custom
dns-manual-placeholder = 1.1.1.1, 1.0.0.1
dns-on-connection = On { $connection }
dns-needs-authorisation = Changing DNS on this connection needs an administrator password.
dns-failed = Could not change DNS: { $reason }

# -- Sliders ------------------------------------------------------------------
volume = Volume
volume-muted = Muted
brightness = Brightness

# -- Shared state -------------------------------------------------------------
on = On
off = Off

# -- Microphone ---------------------------------------------------------------
microphone = Microphone
microphone-muted = Muted

# -- Keyboard backlight -------------------------------------------------------
keyboard-backlight = Keyboard Backlight
keyboard-off = Off
keyboard-low = Low
keyboard-medium = Medium
keyboard-high = High

# -- Media --------------------------------------------------------------------
media = Media
media-nothing-playing = Nothing playing

# -- VPN ----------------------------------------------------------------------
vpn = VPN
vpn-off = Not connected
vpn-add-in-settings = Add or edit VPN connections in Settings.

# -- Other toggles ------------------------------------------------------------
do-not-disturb = Do Not Disturb
keep-awake = Keep Awake
keep-awake-held = Kept awake by { $who }
charge-limit = Limit charging
charge-limit-detail = Stops charging before full, to make the battery last longer.

# -- Settings window ----------------------------------------------------------
settings = Settings
close = Close

tab-tiles = Tiles
tab-styling = Styling
tab-about = About
settings-window-title = Control Center Settings

settings-controls = Controls
settings-preview-detail = This is the grid the panel button opens. Tap a tile to pick it up, move across the grid to see where it will land, tap again to drop it. Switch a tile off to hide it. The words on the tiles are placeholders; the popup shows live state.
settings-controls-detail = Choose what appears in the popup. Some controls only show up if your machine has the hardware for them.

settings-custom = Your tiles
settings-custom-detail = Tiles that run a command of your choosing — a screenshot, a script, anything. Add them to config.toml; the file explains each option and has an example to copy.

settings-style = Tile style
settings-style-detail = How strongly a tile shows that its control is on.
style-high = High contrast
style-high-detail = The whole tile fills with the accent colour when the control is on.
style-medium = Medium contrast
style-medium-detail = Only the icon lights up when the control is on.
style-low = Low contrast
style-low-detail = Tiles never look switched on. You see what is on after opening it.

settings-icon = Panel icon
settings-icon-detail = What the button on the panel shows.
icon-system = System default
icon-custom = Your own
icon-custom-placeholder = Icon name, or a path to an image
icon-select = Select…
icon-choose-title = Choose a panel icon
icon-copied-detail = A picked image is copied into the applet's own folder, so it keeps working if you move or delete the original.
icon-preview = Preview
preset-sliders = Sliders
preset-toggles = Switches
preset-dials = Faders
preset-grid = Tiles

open-config-folder = Open config folder
about-author = jjnuthuagen
about-comments = For COSMIC. Made in Norway with love.
about-source = Source code
about-issues = Report a problem

settings-saved = Changes are saved as you make them.
settings-save-failed = Could not save: { $reason }

# -- Quick toggles ------------------------------------------------------------
dark-mode = Dark Mode
mode-dark = Dark
mode-light = Light
tiling = Window Tiling
tiling-on = Tiled
tiling-off = Floating
