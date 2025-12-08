param(
    [switch]$LoadFunctionsOnly
)

#region Configuration et Initialisation
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
$PSDefaultParameterValues['Out-File:Encoding'] = 'utf8'

$scriptRoot = $PSScriptRoot
$projectRoot = Join-Path $scriptRoot "..\.." | Resolve-Path

$config = @{
    ProgramName   = "Aura-Update"
    DeveloperName = "SPUTJI"
    Version       = "0.3.0"
    ProjectRoot   = $projectRoot
    LogDirectory  = Join-Path $projectRoot "logs"
    UiPath        = Join-Path $projectRoot "ui"
    ProgressDir   = Join-Path $projectRoot "progress"
    Port          = 8080
    UrlPrefix     = "http://localhost:8080/"
    OnWindows     = [System.Runtime.InteropServices.RuntimeInformation]::IsOSPlatform([System.Runtime.InteropServices.OSPlatform]::Windows)
    OnLinux       = [System.Runtime.InteropServices.RuntimeInformation]::IsOSPlatform([System.Runtime.InteropServices.OSPlatform]::Linux)
}
$config.LogPath = Join-Path $config.LogDirectory "Aura-Update_Log.txt"
$config.ErrorLogPath = Join-Path $config.LogDirectory "Aura-Update_Errors.txt"

if (-not (Test-Path $config.LogDirectory)) { New-Item -Path $config.LogDirectory -ItemType Directory -Force | Out-Null }
if (Test-Path $config.ProgressDir) { Remove-Item -Recurse -Force $config.ProgressDir }
New-Item -Path $config.ProgressDir -ItemType Directory -Force | Out-Null

$logMutex = New-Object System.Threading.Mutex($false, "Global\AuraUpdateLogMutex")

function Write-Log {
    param([string]$Message, [string]$File)
    if (-not $File) { $File = $config.LogPath }
    $ts = (Get-Date).ToString("yyyy-MM-dd HH:mm:ss")
    $line = "[$ts] $Message`n"
    try {
        $line | Out-File -FilePath $File -Append -Encoding utf8
        Write-Host $line.TrimEnd()
    } catch {
        Write-Host "[$ts] Erreur de log: $_"
    }
}

Write-Log "Démarrage $($config.ProgramName) v$($config.Version) sur plateforme: $(if ($config.OnWindows) { 'Windows' } else { 'Linux' })"
#endregion

#region Fonctions Utilitaires
function Test-Command([string]$cmd){ try { Get-Command $cmd -ErrorAction Stop -ErrorVariable +Errors } catch { return $false }; return $true }

function Test-Admin {
    if ($config.OnWindows) {
        return ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
    } else {
        return (id -u) -eq 0
    }
}

function Write-ProgressJson {
    param([string]$TaskId, [hashtable]$Payload)
    try {
        $progressFile = Join-Path $config.ProgressDir "$TaskId.json"
        $Payload | ConvertTo-Json -Depth 6 | Out-File -FilePath $progressFile -Encoding utf8
    } catch { Write-Log "Erreur Write-ProgressJson: $_" $config.ErrorLogPath }
}
#endregion

#region Fonctions de Mise à Jour
function Get-WindowsUpdates {
    if (-not (Test-Command 'winget')) { Write-Log "winget introuvable"; return @() }
    $out = & winget upgrade --include-unknown --disable-interactivity 2>&1
    $headerSkipped = $false
    foreach ($line in ($out -split "`n")) {
        $l = $line.Trim()
        if (-not $headerSkipped) { if ($l -match '---') { $headerSkipped = $true }; continue }
        if ($l -match '^(.+?)\s{2,}([^\s]+)\s{2,}([^\s]+)\s{2,}([^\s]+)') {
            $package = [PSCustomObject]@{ Name = $Matches[1].Trim(); Id = $Matches[2].Trim(); Current = $Matches[3].Trim(); Available = $Matches[4].Trim(); Manager = "winget" }
            if ($package.Id -eq 'Id' -or $package.Id -eq 'ID') { continue }
            Write-Log "Package winget parsé: Name='$($package.Name)', Id='$($package.Id)'"
            $package
        }
    }
}

function Get-LinuxUpdates {
    foreach ($manager in @('apt', 'dnf', 'pacman', 'snap', 'flatpak')) {
        if (Test-Command $manager) {
            try {
                switch ($manager) {
                    'apt'     { & bash -c "apt list --upgradable 2>/dev/null" | ForEach-Object { if ($_ -match '^([^/]+)/[^\s]+\s+([^\s]+)\s+\[.+:\s*([^\]]+)\]') { [PSCustomObject]@{ Name=$Matches[1]; Id=$Matches[1]; Current=$Matches[3]; Available=$Matches[2]; Manager="apt" } } } }
                    'dnf'     { & dnf check-update 2>$null | ForEach-Object { if ($_ -match '^\s*([^\s]+)\s+([^\s]+)\s') { [PSCustomObject]@{ Name=$Matches[1]; Id=$Matches[1]; Current=""; Available=$Matches[2]; Manager="dnf" } } } }
                    'pacman'  { & pacman -Qu 2>$null | ForEach-Object { if ($_ -match '^([^\s]+)\s+([^\s]+)') { [PSCustomObject]@{ Name=$Matches[1]; Id=$Matches[1]; Current=""; Available=$Matches[2]; Manager="pacman" } } } }
                    'snap'    { & snap refresh --list 2>$null | ForEach-Object { if ($_ -match '^\s*([^\s]+)\s') { [PSCustomObject]@{ Name=$Matches[1]; Id=$Matches[1]; Current=""; Available=""; Manager="snap" } } } }
                    'flatpak' { & flatpak update --app --assumeyes --dry-run 2>$null | ForEach-Object { if ($_ -match '^•\s+([^\s]+)') { [PSCustomObject]@{ Name=$Matches[1]; Id=$Matches[1]; Current=""; Available=""; Manager="flatpak" } } } }
                }
            } catch { Write-Log "Erreur lors de la vérification avec $manager" $config.ErrorLogPath }
        }
    }
}

function Get-AllUpdates {
    if ($config.OnWindows) { return Get-WindowsUpdates }
    else { return Get-LinuxUpdates }
}

function Update-Package {
    param([string]$Id, [string]$Manager, [string]$TaskId)
    Write-Log "Demande de mise à jour: $Id via $Manager (Tâche: $TaskId)"
    Write-ProgressJson -TaskId $TaskId -Payload @{ state="running"; percent=5; message="Préparation..."; item=$Id; manager=$Manager }
    try {
        $cmd = ""
        $argsList = @()

        switch ($Manager) {
            'winget'  { 
                $cmd = 'winget'
                $argsList = @('upgrade', '--id', $Id, '--accept-package-agreements', '--accept-source-agreements', '--silent', '--disable-interactivity')
            }
            'apt'     { 
                $cmd = 'sudo'
                $argsList = @('apt-get', 'install', '--only-upgrade', '-y', $Id)
            }
            'dnf'     { 
                $cmd = 'sudo'
                $argsList = @('dnf', 'upgrade', '-y', $Id)
            }
            'pacman'  { 
                $cmd = 'sudo'
                $argsList = @('pacman', '-S', '--noconfirm', $Id)
            }
            'snap'    { 
                $cmd = 'sudo'
                $argsList = @('snap', 'refresh', $Id)
            }
            'flatpak' { 
                $cmd = 'flatpak'
                $argsList = @('update', '-y', $Id)
            }
            default   { throw "Gestionnaire inconnu: $Manager" }
        }
        
        Write-ProgressJson -TaskId $TaskId -Payload @{ state="running"; percent=20; message="Mise à jour en cours..."; item=$Id; manager=$Manager }
        
        $output = & $cmd @argsList 2>&1
        $success = $LASTEXITCODE -eq 0

        if ($success) {
            Write-ProgressJson -TaskId $TaskId -Payload @{ state='finished'; percent=100; message='Terminé'; item=$Id; manager=$Manager }
        } else {
            $outputString = $output | Out-String
            $errorMessage = "Erreur: " + ($outputString.Trim() -split "`n" | Select-Object -Last 1)
            Write-Log "Échec de la mise à jour de $Id via $Manager. $errorMessage" $config.ErrorLogPath
            Write-ProgressJson -TaskId $TaskId -Payload @{ state='error'; percent=100; message=$errorMessage; item=$Id; manager=$Manager }
        }

    } catch {
        Write-Log "Exception lors de la mise à jour de ${Id}: $($_.Exception.Message)" $config.ErrorLogPath
        Write-ProgressJson -TaskId $TaskId -Payload @{ state="error"; percent=100; message="Exception: $($_.Exception.Message)"; item=$Id; manager=$Manager }
    }
}

function Update-All {
    param([string]$TaskId)
    Write-Log "Lancement mise à jour complète (Tâche: $TaskId)"
    Write-ProgressJson -TaskId $TaskId -Payload @{ state="running"; percent=5; message="Préparation mise à jour complète"; item="all"; manager="all" }
    
    $commands = @{}
    if ($config.OnWindows) {
        if (Test-Command 'winget') { $commands['winget'] = { & winget upgrade --all --accept-package-agreements --accept-source-agreements --silent --disable-interactivity --include-unknown 2>&1 } }
    } else {
        if (Test-Command 'apt')     { $commands['apt']     = { & sudo apt-get upgrade -y 2>&1 } }
        if (Test-Command 'dnf')     { $commands['dnf']     = { & sudo dnf upgrade -y 2>&1 } }
        if (Test-Command 'pacman')  { $commands['pacman']  = { & sudo pacman -Syu --noconfirm 2>&1 } }
        if (Test-Command 'snap')    { $commands['snap']    = { & sudo snap refresh 2>&1 } }
        if (Test-Command 'flatpak') { $commands['flatpak'] = { & flatpak update -y 2>&1 } }
    }

    $totalManagers = $commands.Count
    $completed = 0
    foreach ($manager in $commands.Keys) {
        $percent = 10 + (80 * ($completed / $totalManagers))
        Write-ProgressJson -TaskId $TaskId -Payload @{ state="running"; percent=$percent; message="Mise à jour avec $manager..."; item="all"; manager=$manager }
        Write-Log "Lancement de la mise à jour globale pour le gestionnaire: $manager"
        $output = & $commands[$manager]
        if ($LASTEXITCODE -eq 0) {
            Write-Log "Mise à jour globale avec $manager terminée avec succès."
        } else {
            Write-Log "Erreur lors de la mise à jour globale avec $manager. Sortie: $output" $config.ErrorLogPath
        }
        $completed++
    }

    Write-Log "Toutes les mises à jour globales sont terminées (Tâche: $TaskId)."
    Write-ProgressJson -TaskId $TaskId -Payload @{ state="finished"; percent=100; message="Mise à jour complète terminée"; item="all"; manager="multiple" }
} 
#endregion

# Si le script est appelé avec -LoadFunctionsOnly, on s'arrête ici.
if ($LoadFunctionsOnly) { return }

#region Serveur HTTP
$listener = New-Object System.Net.HttpListener
[void]$listener.Prefixes.Add($config.UrlPrefix)

try {
    [void]$listener.Start()
    Write-Host "AURA_BACKEND_READY"
} catch {
    Write-Log "FATAL: Impossible de démarrer le serveur HTTP sur $($config.UrlPrefix) : $_"
    exit 1
}

while ($listener.IsListening) {
    try {
        Write-Log "Attente d'une nouvelle requête..."
        $context = $listener.GetContext()
        $request = $context.Request
        $response = $context.Response
        $response.Headers.Add("Access-Control-Allow-Origin", "*")
        $path = [System.Uri]::UnescapeDataString($request.Url.AbsolutePath.TrimStart('/'))

        Write-Log "Requête reçue: $($request.HttpMethod) $path"
        if ($path.StartsWith("api/")) {
            $response.ContentType = "application/json; charset=utf-8"
            $jsonResponse = ""

            switch -Wildcard ($path) {
                "api/check" {
                    $jsonResponse = Get-AllUpdates | ConvertTo-Json -Depth 4
                }
                "api/update-all" {
                    if ($request.HttpMethod -eq "POST") {
                        $taskId = [guid]::NewGuid().ToString()
                        $jobScriptBlock = {
                            param($taskId, $config, $logMutex)
                            ${function:Write-Log} = ${using:function:Write-Log}
                            ${function:Write-ProgressJson} = ${using:function:Write-ProgressJson}
                            ${function:Update-All} = ${using:function:Update-All}
                            ${function:Test-Command} = ${using:function:Test-Command}

                            Update-All -TaskId $taskId
                        }
                        Start-Job -ScriptBlock $jobScriptBlock -ArgumentList $taskId, $config, $logMutex | Out-Null
                        $jsonResponse = @{ taskId = $taskId } | ConvertTo-Json
                    }
                }
                "api/update" {
                     if ($request.HttpMethod -eq "POST") {
                        $body = New-Object System.IO.StreamReader -ArgumentList $request.InputStream, $request.ContentEncoding
                        $data = $body.ReadToEnd() | ConvertFrom-Json
                        $taskId = [guid]::NewGuid().ToString()
                        $jobScriptBlock = {
                            param($id, $mgr, $taskId, $config, $logMutex)
                            ${function:Write-Log} = ${using:function:Write-Log}
                            ${function:Write-ProgressJson} = ${using:function:Write-ProgressJson}
                            ${function:Update-Package} = ${using:function:Update-Package}
                            
                            Update-Package -Id $id -Manager $mgr -TaskId $taskId
                        }
                        Start-Job -ScriptBlock $jobScriptBlock -ArgumentList $data.id, $data.mgr, $taskId, $config, $logMutex | Out-Null
                        $jsonResponse = @{ taskId = $taskId } | ConvertTo-Json
                    }
                }
                "api/progress" {
                    $tasks = @{ }
                    Get-ChildItem -Path $config.ProgressDir -Filter "*.json" | ForEach-Object {
                        $tasks[$_.BaseName] = Get-Content $_.FullName -Raw | ConvertFrom-Json
                    }
                    $jsonResponse = $tasks | ConvertTo-Json -Depth 5
                }
                "api/admin-status" {
                    $isAdmin = Test-Admin
                    $jsonResponse = @{ isAdmin = $isAdmin; platform = $(if ($config.OnWindows) { 'Windows' } else { 'Linux' }) } | ConvertTo-Json
                }
                "api/elevate" {
                    if ($request.HttpMethod -eq "POST") {
                        if ($config.OnWindows) {
                            try {
                                Write-Host "AURA_REQUEST_RELAUNCH_AS_ADMIN"
                                $jsonResponse = @{ message = "Demande de relance en mode administrateur envoyée." } | ConvertTo-Json
                            } catch {
                                $response.StatusCode = 500
                                $jsonResponse = @{ error = "Échec de l'élévation: $($_.Exception.Message)" } | ConvertTo-Json
                            }
                        } else {
                            $response.StatusCode = 400
                            $jsonResponse = @{ error = "Élévation non supportée sur cette plateforme" } | ConvertTo-Json
                        }
                    } else {
                        $response.StatusCode = 405
                        $jsonResponse = @{ error = "Method not allowed" } | ConvertTo-Json
                    }
                }
                default {
                    $response.StatusCode = 404
                    $jsonResponse = @{ error = "API route not found" } | ConvertTo-Json
                }
            }
            if ($listener.IsListening) {
                $bytes = [System.Text.Encoding]::UTF8.GetBytes($jsonResponse)
                $response.OutputStream.Write($bytes, 0, $bytes.Length)
            }
        }
        else {
            $requestedFile = if ([string]::IsNullOrEmpty($path)) { "index.html" } else { $path }
            $filePath = Join-Path $config.UiPath $requestedFile

            if (Test-Path $filePath -PathType Leaf) {
                $ext = [System.IO.Path]::GetExtension($filePath).ToLower()
                $mime = @{
                    ".html" = "text/html; charset=utf-8"
                    ".css"  = "text/css"
                    ".js"   = "application/javascript"
                    ".png"  = "image/png"
                    ".svg"  = "image/svg+xml"
                    ".ico"  = "image/x-icon"
                }[$ext]

                if ($mime) { $response.ContentType = $mime }

                $fileBytes = [System.IO.File]::ReadAllBytes($filePath)
                $response.OutputStream.Write($fileBytes, 0, $fileBytes.Length)
            } else {
                $response.StatusCode = 404
            }
        }
    } catch {
        Write-Log "ERREUR dans la boucle du serveur: $($_.Exception.Message)" $config.ErrorLogPath
        if ($response -and $response.OutputStream.CanWrite) {
            $response.StatusCode = 500
        }
    } finally {
        if ($response -and $listener.IsListening) { $response.Close() }
    }
}
#endregion