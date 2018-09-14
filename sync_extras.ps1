if (!(Test-Path -Path extras/pathfinder)) { git clone https://github.com/pcwalton/pathfinder extras/pathfinder }
else { Set-Location extras/pathfinder; git pull -ff; Set-Location ../.. }
