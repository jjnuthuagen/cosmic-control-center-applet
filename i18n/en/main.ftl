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
power-profile = Power profile
profile-power-saver = Power Saver
profile-balanced = Balanced
profile-performance = Performance
performance-degraded = Performance limited: { $reason }

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

# -- Quick toggles ------------------------------------------------------------
mode-dark = Dark
mode-light = Light
tiling-on = Tiled
tiling-off = Floating
