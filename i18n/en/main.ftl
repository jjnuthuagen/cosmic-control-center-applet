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
game-mode-detail = Feral GameMode — stacks with the profile above
game-mode-held = Held on by a running game

# -- DNS ----------------------------------------------------------------------
dns = DNS
dns-automatic = Automatic
dns-cloudflare = Cloudflare
dns-google = Google
dns-quad9 = Quad9
dns-custom = Custom
dns-manual-placeholder = 1.1.1.1, 1.0.0.1
dns-on-connection = On { $connection }
dns-needs-authorisation = This connection is managed system-wide, so changing its DNS needs an administrator password.
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
keep-awake-held = Held by { $who }
charge-limit = Limit charging
charge-limit-detail = Stop charging before full to reduce battery wear.

# -- Settings window ----------------------------------------------------------
settings = Settings
close = Close

settings-controls = Controls
settings-controls-detail = Choose which controls appear in the popup. A control switched off here is never started and never connects to anything. Controls also hide themselves when the hardware or service they need is missing, so switching one on does not guarantee a tile.

settings-custom-detail = Tiles you added yourself, from the [[custom]] entries in config.toml.

settings-style = Tile style
settings-style-detail = How strongly a tile shows that its control is on.
style-high = High contrast
style-high-detail = The whole tile fills with the accent colour when the control is on.
style-medium = Medium contrast
style-medium-detail = The tile stays neutral and the icon sits on a shape that takes the accent colour.
style-low = Low contrast
style-low-detail = Tiles never look selected. You see what is chosen only after opening a control, the way the battery tile already works.

settings-icon = Panel icon
settings-icon-detail = What the button on the panel shows.
icon-system = System default
icon-custom-placeholder = Icon name, or a path to an image
icon-preview = Preview
preset-sliders = Sliders
preset-toggles = Switches
preset-dials = Faders
preset-grid = Tiles

settings-saved = Changes are saved as you make them.
settings-save-failed = Could not save: { $reason }

# -- Quick toggles ------------------------------------------------------------
dark-mode = Dark Mode
mode-dark = Dark
mode-light = Light
tiling = Window Tiling
tiling-on = Tiled
tiling-off = Floating
