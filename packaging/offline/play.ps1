# Starts Horizon from this folder: no launcher, no account. The first run prepares the game's
# resources from your own Minecraft: Java Edition (PLAY.txt), later runs start the game at once.
# play.bat runs this file; arguments go to horizon-client.exe.
#
# HORIZON_PLAYER_NAME   the player name, instead of asking and keeping it in player-name.txt
# HORIZON_MINECRAFT_DIR the official launcher's game folder, when it is not %APPDATA%\.minecraft
# HORIZON_SERVER_JAR    the pinned server jar, when it is not server.jar in this folder
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'

$here = Split-Path -Parent $MyInvocation.MyCommand.Path
$prepare = Join-Path $here 'horizon-prepare.exe'
$client = Join-Path $here 'horizon-client.exe'

function Fail([string]$text) {
    Write-Host ''
    Write-Host $text -ForegroundColor Red
    exit 1
}

$version = $null
$lock = Join-Path $here 'research-lock.json'
if (Test-Path -LiteralPath $lock) {
    $version = (Get-Content -Raw -LiteralPath $lock | ConvertFrom-Json).minecraft.version
}
if (-not $version) {
    Fail 'research-lock.json beside play.bat names no Minecraft version: unpack the archive again.'
}
$resources = Join-Path $here (Join-Path '.local\horizon-client' $version)
$game = Join-Path $here 'game'

# The player name: Minecraft's rule for one, so every server takes it.
$nameFile = Join-Path $here 'player-name.txt'
$fromEnvironment = [bool]$env:HORIZON_PLAYER_NAME
$name = $env:HORIZON_PLAYER_NAME
if (-not $name -and (Test-Path -LiteralPath $nameFile)) {
    $name = [string](Get-Content -LiteralPath $nameFile -TotalCount 1)
}
if ($name) { $name = $name.Trim() }
while ($name -notmatch '^[A-Za-z0-9_]{3,16}$') {
    if ($name) {
        Write-Host ('"{0}" is not a player name: 3 to 16 letters, digits or underscores.' -f $name)
    }
    try { $name = Read-Host 'Player name' } catch { $name = $null }
    if ($null -eq $name) {
        Fail 'No player name. Set HORIZON_PLAYER_NAME or write one into player-name.txt.'
    }
    $name = $name.Trim()
}
if (-not $fromEnvironment) {
    Set-Content -LiteralPath $nameFile -Value $name -Encoding ASCII
}

# The pinned server jar, where the player gave none: the address their own launcher's version
# file names (Mojang's public download). horizon-prepare checks its checksum before using it.
function Find-ServerJar {
    if ($env:HORIZON_SERVER_JAR) { return }
    $jar = Join-Path $here 'server.jar'
    if (-not (Test-Path -LiteralPath $jar)) {
        $minecraft = $env:HORIZON_MINECRAFT_DIR
        if (-not $minecraft) { $minecraft = Join-Path $env:APPDATA '.minecraft' }
        $json = Join-Path $minecraft (Join-Path 'versions' (Join-Path $version "$version.json"))
        if (-not (Test-Path -LiteralPath $json)) { return }
        $url = $null
        try {
            $url = (Get-Content -Raw -LiteralPath $json | ConvertFrom-Json).downloads.server.url
        } catch {
            return
        }
        if (-not $url -or $url -notmatch '^https://[a-z0-9.-]+\.mojang\.com/') { return }
        Write-Host "Downloading the Minecraft $version server jar from $url"
        try {
            [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
            Invoke-WebRequest -Uri $url -OutFile "$jar.part" -UseBasicParsing
            Move-Item -LiteralPath "$jar.part" -Destination $jar -Force
        } catch {
            Write-Host "The download failed: $($_.Exception.Message)"
            return
        }
    }
    $env:HORIZON_SERVER_JAR = $jar
}

Write-Host 'Checking the prepared resources'
& $prepare --check
if ($LASTEXITCODE -ne 0) {
    Write-Host ''
    Write-Host "Preparing Horizon from your Minecraft $version (a few minutes, once)"
    Find-ServerJar
    & $prepare --prepare --complete
    if ($LASTEXITCODE -ne 0) {
        if (-not (Test-Path -LiteralPath (Join-Path $resources 'horizon-resources.json'))) {
            Fail 'Preparing failed: the lines above say what is missing. PLAY.txt lists what the first run needs.'
        }
        Write-Host ''
        try {
            $answer = Read-Host 'The resources are incomplete (see above). Start the game anyway? [y/N]'
        } catch {
            $answer = 'n'
        }
        if ($answer -notmatch '^[yY]') { exit 1 }
    }
}

New-Item -ItemType Directory -Force -Path $game | Out-Null
$arguments = @('--resources', $resources, '--game-dir', $game, '--home', $game, '--name', $name) + $args
& $client @arguments
if ($null -ne $LASTEXITCODE) { exit $LASTEXITCODE }
