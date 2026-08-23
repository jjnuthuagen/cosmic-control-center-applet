# Control Center — English strings.
#
# Every user-facing string in the applet lives here. Keep ids kebab-case and
# grouped by the tile they belong to.

applet-name = Control Center
applet-tooltip = Quick settings

# -- Wi-Fi --------------------------------------------------------------------
wifi = Wi-Fi
wifi-off = Off
wifi-disconnected = Not connected

# -- Bluetooth ----------------------------------------------------------------
bluetooth = Bluetooth
bluetooth-off = Off
bluetooth-no-devices = No devices
bluetooth-devices = { $count ->
    [one] { $count } device
   *[other] { $count } devices
}

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
dns-automatic = Automatic (DHCP)
dns-cloudflare = Cloudflare
dns-google = Google
dns-quad9 = Quad9
dns-custom = Custom
dns-manual-placeholder = 1.1.1.1, 1.0.0.1
dns-manual-apply = Apply
dns-on-connection = On { $connection }
dns-needs-authorisation = This connection is managed system-wide, so changing its DNS needs an administrator password.
dns-failed = Could not change DNS: { $reason }

# -- Sliders ------------------------------------------------------------------
volume = Volume
volume-muted = Muted
brightness = Brightness

# -- Quick toggles ------------------------------------------------------------
dark-mode = Dark Mode

# -- Navigation ---------------------------------------------------------------
back = Back
