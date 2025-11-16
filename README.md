There are plenty of applications that remind you to take breaks, however there aren't many that can block the screen with a pop-up when a break has arrived and none that are Wayland-native, as far as I know. Additionally, the applications that I know of aren't very console-friendly.

There are still a lot of things I plan on supporting, some of them are:
 - add a verbose option to the config to configure the level of log outputs
 - proper packaging (no need for manual copying of files during installation)
 - enable reloading of the config on systemctl reload
 - think of an alternative for the screen blocking (maybe a smaller pop-up?)
 - implement tests


The current sound being used is:
Rebana L Gong by RoNz -- https://freesound.org/s/397942/ -- License: Attribution 3.0


current steps for installation:
1. copy wlbreaktime.service and wlbreaktime.socket to ~/.config/systemd/user/.
2. link the starting to your compositor (or something similar) -- example for niri:
    - `mkdir ~/.config/systemd/user/niri.service.wants`
    - `ln -s ~/.config/systemd/user/wlbreaktime.service ~/.config/systemd/user/niri.service.wants/.`
