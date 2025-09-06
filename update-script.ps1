# Nom du programme : Aura-Update
# Développé par : SPUTJI
# Version : 0.2.14

# --- Définitions ---
$ProgramName = "Aura-Update"
$DeveloperName = "SPUTJI"
$Version = "0.2.14"

# Déterminer le chemin du script ou de l'exécutable
if ($PSCommandPath) {
    $scriptPath = Split-Path -Parent $PSCommandPath
} else {
    $scriptPath = [System.AppDomain]::CurrentDomain.BaseDirectory.TrimEnd('\')
}
if (-not $scriptPath -or $scriptPath -eq "") {
    [System.Windows.Forms.MessageBox]::Show("Erreur : Impossible de déterminer le chemin du script ou de l'exécutable.", "Erreur Critique", [System.Windows.Forms.MessageBoxButtons]::OK, [System.Windows.Forms.MessageBoxIcon]::Error)
    Start-Sleep -Seconds 5
    exit
}

# Chemin des logs
$logDirectory = Join-Path $scriptPath "logs"
$logPath = Join-Path $logDirectory "Aura-Update_Log.txt"
$errorLogPath = Join-Path $logDirectory "Aura-Update_Errors.txt"
$exePath = Join-Path $scriptPath "Aura-Update.exe"

# --- Gestion du fichier de log avec encodage UTF-8 ---
function Write-Log {
    param (
        [string]$Message,
        [string]$LogFile = $logPath
    )
    if (-not $Message -or $Message.Trim() -eq "") {
        return
    }
    $timestamp = Get-Date -Format "yyyy-MM-dd HH:mm:ss"
    $logMessage = "[$timestamp] $Message"
    try {
        if (-not (Test-Path $LogFile)) {
            New-Item -Path $LogFile -ItemType File -Force | Out-Null
        }
        [System.IO.File]::AppendAllText($LogFile, "$logMessage`n", [System.Text.Encoding]::UTF8)
    } catch {
        [System.Windows.Forms.MessageBox]::Show("Erreur d'écriture dans le log : $_", "Erreur", [System.Windows.Forms.MessageBoxButtons]::OK, [System.Windows.Forms.MessageBoxIcon]::Warning)
    }
}

# --- Vérifier la version de .NET Framework ---
try {
    $dotNetVersion = (Get-ItemProperty -Path "HKLM:\SOFTWARE\Microsoft\NET Framework Setup\NDP\v4\Full" -ErrorAction Stop).Version
    if ($dotNetVersion -lt "4.5") {
        Write-Log "Erreur : .NET Framework 4.5 ou supérieur requis." $errorLogPath
        [System.Windows.Forms.MessageBox]::Show("Erreur : .NET Framework 4.5 ou supérieur requis. Veuillez l'installer depuis le site de Microsoft.", "Erreur Critique", [System.Windows.Forms.MessageBoxButtons]::OK, [System.Windows.Forms.MessageBoxIcon]::Error)
        exit
    }
    Write-Log ".NET Framework version $dotNetVersion détecté."
} catch {
    Write-Log "Erreur lors de la vérification de .NET Framework : $_" $errorLogPath
    [System.Windows.Forms.MessageBox]::Show("Erreur lors de la vérification de .NET Framework : $_", "Erreur", [System.Windows.Forms.MessageBoxButtons]::OK, [System.Windows.Forms.MessageBoxIcon]::Error)
    exit
}

# --- Vérifier la disponibilité de winget ---
function Test-Winget {
    try {
        $possiblePaths = @(
            "$env:LocalAppData\Microsoft\WindowsApps\winget.exe",
            "C:\Program Files\WindowsApps\Microsoft.DesktopAppInstaller_*\winget.exe"
        )
        $wingetPath = $null
        foreach ($path in $possiblePaths) {
            $resolvedPath = (Get-Item $path -ErrorAction SilentlyContinue).FullName
            if ($resolvedPath) {
                $wingetPath = $resolvedPath
                break
            }
        }
        if (-not $wingetPath) {
            Write-Log "Erreur : Winget n'est pas installé ou introuvable." $errorLogPath
            [System.Windows.Forms.MessageBox]::Show("Erreur : Winget n'est pas installé. Veuillez installer 'App Installer' depuis le Microsoft Store.", "Erreur", [System.Windows.Forms.MessageBoxButtons]::OK, [System.Windows.Forms.MessageBoxIcon]::Error)
            return $false, $null
        }
        $wingetVersion = & $wingetPath --version
        Write-Log "Winget détecté, version : $wingetVersion, chemin : $wingetPath"
        $env:Path += ";$(Split-Path $wingetPath -Parent)"
        return $true, $wingetPath
    } catch {
        Write-Log "Erreur : Winget n'est pas installé ou inaccessible : $_" $errorLogPath
        return $false, $null
    }
}

# --- Vérifier la connectivité réseau ---
function Test-Network {
    try {
        $ping = Test-Connection -ComputerName "8.8.8.8" -Count 1 -Quiet
        if (-not $ping) {
            Write-Log "Erreur : Aucune connectivité réseau détectée." $errorLogPath
            return $false
        }
        return $true
    } catch {
        Write-Log "Erreur lors de la vérification du réseau : $_" $errorLogPath
        return $false
    }
}

# --- Vérifier les droits administrateur ---
function Test-Admin {
    $currentUser = [Security.Principal.WindowsIdentity]::GetCurrent()
    $principal = New-Object Security.Principal.WindowsPrincipal($currentUser)
    return $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
}

# --- Créer le répertoire de logs ---
try {
    if (-not (Test-Path $logDirectory)) {
        New-Item -Path $logDirectory -ItemType Directory -Force | Out-Null
        Write-Log "Répertoire de logs créé : $logDirectory"
    }
} catch {
    Write-Log "Erreur lors de la création du répertoire de logs : $_" $errorLogPath
    [System.Windows.Forms.MessageBox]::Show("Erreur : Impossible de créer le répertoire de logs. Vérifiez les droits d'accès au dossier.", "Erreur Critique", [System.Windows.Forms.MessageBoxButtons]::OK, [System.Windows.Forms.MessageBoxIcon]::Error)
    Start-Sleep -Seconds 5
    exit
}

# --- Charger les modules pour l'interface graphique ---
try {
    Add-Type -AssemblyName System.Windows.Forms
    Add-Type -AssemblyName System.Drawing
} catch {
    Write-Log "Erreur critique : Impossible de charger les modules de l'interface graphique : $_" $errorLogPath
    [System.Windows.Forms.MessageBox]::Show("Erreur critique : Impossible de charger les modules de l'interface graphique.", "Erreur Critique", [System.Windows.Forms.MessageBoxButtons]::OK, [System.Windows.Forms.MessageBoxIcon]::Error)
    Start-Sleep -Seconds 5
    exit
}

# --- Fonctions du programme ---

function Update-Specific {
    param (
        [string]$PackageId,
        [string]$PackageName,
        [System.Windows.Forms.ProgressBar]$DownloadProgress,
        [System.Windows.Forms.ProgressBar]$InstallProgress,
        [System.Windows.Forms.TextBox]$StatusBox
    )
    if (-not $PackageId -or -not $PackageName) {
        Write-Log "Erreur : ID ou nom du package manquant pour la mise à jour." $errorLogPath
        $StatusBox.Text = "Erreur : Informations de package manquantes."
        return
    }
    $StatusBox.Text = "Mise à jour de $PackageName en cours..."
    $StatusBox.Refresh()
    Write-Log "Début de la mise à jour de $PackageName (ID : $PackageId)"

    $DownloadProgress.Value = 0
    $InstallProgress.Value = 0

    try {
        $wingetResult, $wingetPath = Test-Winget
        if (-not $wingetResult) {
            throw "Winget.exe introuvable."
        }

        if (-not (Test-Admin)) {
            Write-Log "Avertissement : Droits administrateur non détectés. Certaines mises à jour peuvent échouer." $errorLogPath
            $StatusBox.Text = "Avertissement : Exécutez l'application en mode administrateur pour les mises à jour système."
            $StatusBox.Refresh()
            Start-Sleep -Seconds 2
            $StatusBox.Text = "Mise à jour de $PackageName en cours..."
            $StatusBox.Refresh()
        }

        $processInfo = New-Object System.Diagnostics.ProcessStartInfo
        $processInfo.FileName = $wingetPath
        $processInfo.Arguments = "upgrade --id `"$PackageId`" --accept-package-agreements --accept-source-agreements --force --silent --disable-interactivity"
        $processInfo.RedirectStandardOutput = $true
        $processInfo.RedirectStandardError = $true
        $processInfo.UseShellExecute = $false
        $processInfo.CreateNoWindow = $true
        $processInfo.StandardOutputEncoding = [System.Text.Encoding]::UTF8
        $processInfo.StandardErrorEncoding = [System.Text.Encoding]::UTF8
        $processInfo.WindowStyle = [System.Diagnostics.ProcessWindowStyle]::Hidden

        $process = [System.Diagnostics.Process]::Start($processInfo)
        
        for ($i = 0; $i -le 50; $i += 5) {
            $DownloadProgress.Value = $i
            $DownloadProgress.Refresh()
            Start-Sleep -Milliseconds 100
        }

        $output = $process.StandardOutput.ReadToEnd()
        $errorOutput = $process.StandardError.ReadToEnd()
        $process.WaitForExit()

        for ($i = 50; $i -le 100; $i += 5) {
            $InstallProgress.Value = $i
            $InstallProgress.Refresh()
            Start-Sleep -Milliseconds 100
        }

        if ($process.ExitCode -eq 0) {
            $StatusBox.Text = "Mise à jour de $PackageName réussie."
            Write-Log "Mise à jour de $PackageName réussie : $output"
        } else {
            $errorMessage = if ($errorOutput) { $errorOutput } else { "Erreur inconnue (code de sortie : $($process.ExitCode))." }
            if ($PackageId -eq "GaijinNetwork.WarThunder") {
                $errorMessage = "War Thunder Launcher ne peut pas être mis à jour via winget (lanceur spécifique)."
            }
            $StatusBox.Text = "Erreur lors de la mise à jour de $PackageName : $errorMessage"
            Write-Log "Erreur lors de la mise à jour de $PackageName : $errorMessage" $errorLogPath
        }
    } catch {
        $StatusBox.Text = "Erreur critique lors de la mise à jour de $PackageName : $_"
        Write-Log "Erreur critique lors de la mise à jour de $PackageName : $_" $errorLogPath
    }

    $DownloadProgress.Value = 100
    $InstallProgress.Value = 100
    $DownloadProgress.Refresh()
    $InstallProgress.Refresh()
    Check-Updates
}

function Check-Updates {
    $listPanel.Controls.Clear()
    $resultBox.Text = "Recherche des mises à jour disponibles..."
    $resultBox.Refresh()
    Write-Log "Recherche des mises à jour..."

    if (-not (Test-Winget)) {
        $resultBox.Text = "Erreur : Winget n'est pas installé ou inaccessible. Vérifiez votre installation."
        $updateAllButton.Enabled = $false
        Write-Log "Échec de la vérification des mises à jour : Winget non disponible." $errorLogPath
        return
    }

    if (-not (Test-Network)) {
        $resultBox.Text = "Erreur : Aucune connectivité réseau. Vérifiez votre connexion."
        $updateAllButton.Enabled = $false
        return
    }

    try {
        $wingetResult, $wingetPath = Test-Winget
        if (-not $wingetResult) {
            throw "Winget.exe introuvable."
        }

        $processInfo = New-Object System.Diagnostics.ProcessStartInfo
        $processInfo.FileName = $wingetPath
        $processInfo.Arguments = "upgrade --include-unknown --disable-interactivity"
        $processInfo.RedirectStandardOutput = $true
        $processInfo.RedirectStandardError = $true
        $processInfo.UseShellExecute = $false
        $processInfo.CreateNoWindow = $true
        $processInfo.StandardOutputEncoding = [System.Text.Encoding]::UTF8
        $processInfo.StandardErrorEncoding = [System.Text.Encoding]::UTF8
        $processInfo.WindowStyle = [System.Diagnostics.ProcessWindowStyle]::Hidden

        $process = [System.Diagnostics.Process]::Start($processInfo)
        $wingetOutput = $process.StandardOutput.ReadToEnd()
        $errorOutput = $process.StandardError.ReadToEnd()
        $process.WaitForExit()

        Write-Log "Sortie brute de winget : $wingetOutput"
        
        if ($errorOutput) {
            Write-Log "Sortie d'erreur de winget : $errorOutput" $errorLogPath
        }

        $lines = $wingetOutput.Split("`n") | Where-Object { $_.Trim() -ne "" }
    } catch {
        $resultBox.Text = "Erreur lors de la récupération des mises à jour : $_"
        $updateAllButton.Enabled = $false
        Write-Log "Erreur lors de la récupération des mises à jour : $_" $errorLogPath
        return
    }

    $updatesFound = $false
    $wingetHeaderSkipped = $false
    $headers = @("Nom", "ID", "Version Actuelle", "Nouvelle Version", "Action")
    $columnPositions = @(10, 160, 340, 440, 540)
    $yPosition = 30

    for ($i = 0; $i -lt $headers.Count; $i++) {
        $headerLabel = New-Object System.Windows.Forms.Label
        $headerLabel.Text = $headers[$i]
        $headerLabel.Font = New-Object System.Drawing.Font("Segoe UI", 10, [System.Drawing.FontStyle]::Bold)
        $headerLabel.ForeColor = [System.Drawing.Color]::White
        $headerLabel.Location = New-Object System.Drawing.Point($columnPositions[$i], 5)
        $headerLabel.Size = New-Object System.Drawing.Size(100, 20)
        $listPanel.Controls.Add($headerLabel)
    }

    $excludedPackages = @("Microsoft.Edge", "Microsoft.VCRedist.2013.x64", "Microsoft.VCRedist.2013.x86")

    foreach ($line in $lines) {
        if (-not $wingetHeaderSkipped) {
            if ($line.Trim() -match '---') {
                $wingetHeaderSkipped = $true
            }
            continue
        }

        if ($line.Trim() -match 'package' -or $line.Trim() -match '\(' -or $line.Trim() -match 'installing' -or $line.Trim() -match 'Trouv' -or $line.Trim() -match 'La licence' -or $line.Trim() -match '^[-/|\\]+$' -or $line.Trim() -match '^\s*Nom\s*ID\s*Version\s*Disponible\s*Source\s*$') {
            continue
        }

        $parts = $line.Trim() -split '\s{2,}'
        if ($parts.Count -lt 4 -or $parts.Count -gt 5) {
            Write-Log "Ligne ignorée (format invalide) : $line"
            continue
        }

        $name = $parts[0]
        $id = $parts[1]
        $currentVersion = $parts[2]
        $availableVersion = $parts[3]

        if ($name -eq '' -or $name -match '^[-/|\\]+$' -or $id -eq '' -or $id -match '^[-/|\\]+$' -or $currentVersion -match '^[-/|\\]+$' -or $availableVersion -match '^[-/|\\]+$') {
            Write-Log "Entrée ignorée (illisible ou inaccessible) : $line"
            continue
        }

        if ($excludedPackages -contains $id) {
            Write-Log "Mise à jour ignorée pour $name (ID: $id) : Package système non pris en charge."
            continue
        }

        $updatesFound = $true

        $nameLabel = New-Object System.Windows.Forms.Label
        $nameLabel.Text = $name
        $nameLabel.ForeColor = [System.Drawing.Color]::White
        $nameLabel.Location = New-Object System.Drawing.Point(10, $yPosition)
        $nameLabel.Size = New-Object System.Drawing.Size(140, 20)
        $listPanel.Controls.Add($nameLabel)

        $idLabel = New-Object System.Windows.Forms.Label
        $idLabel.Text = $id
        $idLabel.ForeColor = [System.Drawing.Color]::White
        $idLabel.Location = New-Object System.Drawing.Point(160, $yPosition)
        $idLabel.Size = New-Object System.Drawing.Size(170, 20)
        $listPanel.Controls.Add($idLabel)

        $currentVersionLabel = New-Object System.Windows.Forms.Label
        $currentVersionLabel.Text = $currentVersion
        $currentVersionLabel.ForeColor = [System.Drawing.Color]::White
        $currentVersionLabel.Location = New-Object System.Drawing.Point(340, $yPosition)
        $currentVersionLabel.Size = New-Object System.Drawing.Size(90, 20)
        $listPanel.Controls.Add($currentVersionLabel)

        $availableVersionLabel = New-Object System.Windows.Forms.Label
        $availableVersionLabel.Text = $availableVersion
        $availableVersionLabel.ForeColor = [System.Drawing.Color]::White
        $availableVersionLabel.Location = New-Object System.Drawing.Point(440, $yPosition)
        $availableVersionLabel.Size = New-Object System.Drawing.Size(90, 20)
        $listPanel.Controls.Add($availableVersionLabel)

        $updateButton = New-Object System.Windows.Forms.Button
        $updateButton.Text = "Mettre à jour"
        $updateButton.Location = New-Object System.Drawing.Point(540, $yPosition)
        $updateButton.Size = New-Object System.Drawing.Size(100, 25)
        $updateButton.BackColor = [System.Drawing.Color]::FromArgb(30, 144, 255)
        $updateButton.ForeColor = [System.Drawing.Color]::White
        $updateButton.FlatStyle = [System.Windows.Forms.FlatStyle]::Flat
        $updateButton.FlatAppearance.BorderSize = 0
        $updateButton.Tag = @{ PackageId = $id; PackageName = $name }
        $updateButton.Add_Click({
            param($sender)
            Update-Specific -PackageId $sender.Tag.PackageId -PackageName $sender.Tag.PackageName -DownloadProgress $downloadProgress -InstallProgress $installProgress -StatusBox $resultBox
        })
        $listPanel.Controls.Add($updateButton)

        $yPosition += 30
        Write-Log "Mise à jour disponible pour $name (ID: $id, Version actuelle: $currentVersion, Nouvelle version: $availableVersion)"
    }

    if ($updatesFound) {
        $updateAllButton.Enabled = $true
        $resultBox.Text = "Des mises à jour sont disponibles. Sélectionnez un programme ou mettez tout à jour."
    } else {
        $resultBox.Text = "Tous les programmes gérés par Winget sont à jour."
        $updateAllButton.Enabled = $false
    }
}

function Update-All {
    $resultBox.Text = "Mise à jour de tous les programmes en cours..."
    $resultBox.Refresh()
    Write-Log "Lancement de la mise à jour complète..."

    $downloadProgress.Value = 0
    $installProgress.Value = 0

    try {
        $wingetResult, $wingetPath = Test-Winget
        if (-not $wingetResult) {
            throw "Winget.exe introuvable."
        }

        if (-not (Test-Admin)) {
            Write-Log "Avertissement : Droits administrateur non détectés. Certaines mises à jour peuvent échouer." $errorLogPath
            $resultBox.Text = "Avertissement : Exécutez l'application en mode administrateur pour les mises à jour système."
            $resultBox.Refresh()
            Start-Sleep -Seconds 2
            $resultBox.Text = "Mise à jour de tous les programmes en cours..."
            $resultBox.Refresh()
        }

        $processInfo = New-Object System.Diagnostics.ProcessStartInfo
        $processInfo.FileName = $wingetPath
        $processInfo.Arguments = "upgrade --all --accept-package-agreements --accept-source-agreements --force --silent --disable-interactivity --include-unknown"
        $processInfo.RedirectStandardOutput = $true
        $processInfo.RedirectStandardError = $true
        $processInfo.UseShellExecute = $false
        $processInfo.CreateNoWindow = $true
        $processInfo.StandardOutputEncoding = [System.Text.Encoding]::UTF8
        $processInfo.StandardErrorEncoding = [System.Text.Encoding]::UTF8
        $processInfo.WindowStyle = [System.Diagnostics.ProcessWindowStyle]::Hidden

        $process = [System.Diagnostics.Process]::Start($processInfo)
        
        for ($i = 0; $i -le 50; $i += 5) {
            $DownloadProgress.Value = $i
            $DownloadProgress.Refresh()
            Start-Sleep -Milliseconds 200
        }

        $output = $process.StandardOutput.ReadToEnd()
        $errorOutput = $process.StandardError.ReadToEnd()
        $process.WaitForExit()

        for ($i = 50; $i -le 100; $i += 5) {
            $InstallProgress.Value = $i
            $InstallProgress.Refresh()
            Start-Sleep -Milliseconds 200
        }

        $successCount = 0
        $failureCount = 0
        $lines = $output.Split("`n")
        foreach ($line in $lines) {
            if ($line -match "Installé correctement") {
                $successCount++
            } elseif ($line -match "Une erreur inattendue") {
                $failureCount++
            }
        }

        $resultBox.Text = "Mise à jour terminée : $successCount programmes mis à jour avec succès, $failureCount erreurs (ignorées). Voir les logs."
        Write-Log "Mise à jour complète : $successCount réussites, $failureCount erreurs. Sortie : $output"
        if ($errorOutput) {
            Write-Log "Erreurs rencontrées : $errorOutput" $errorLogPath
        }
    } catch {
        $resultBox.Text = "Erreur critique lors de la mise à jour complète : $_"
        Write-Log "Erreur critique lors de la mise à jour complète : $_" $errorLogPath
    }

    $DownloadProgress.Value = 100
    $InstallProgress.Value = 100
    $DownloadProgress.Refresh()
    $InstallProgress.Refresh()
    Check-Updates
}

# --- Création de l'interface graphique moderne ---
$form = New-Object System.Windows.Forms.Form
$form.Text = "$ProgramName - v$Version"
$form.Size = New-Object System.Drawing.Size(800, 700)
$form.StartPosition = "CenterScreen"
$form.Font = New-Object System.Drawing.Font("Segoe UI", 10)
$form.FormBorderStyle = "FixedSingle"
$form.MaximizeBox = $false
$form.BackColor = [System.Drawing.Color]::FromArgb(28, 28, 28)

# Utiliser l'icône locale favicon.ico
$iconPath = Join-Path $scriptPath "favicon.ico"
try {
    if (Test-Path $iconPath) {
        $form.Icon = [System.Drawing.Icon]::new($iconPath)
    } else {
        Write-Log "Icône favicon.ico introuvable dans $scriptPath" $errorLogPath
    }
} catch {
    Write-Log "Erreur lors du chargement de l'icône : $_" $errorLogPath
}

$titleLabel = New-Object System.Windows.Forms.Label
$titleLabel.Text = "$ProgramName"
$titleLabel.Font = New-Object System.Drawing.Font("Segoe UI", 18, [System.Drawing.FontStyle]::Bold)
$titleLabel.ForeColor = [System.Drawing.Color]::White
$titleLabel.AutoSize = $true
$titleLabel.Location = New-Object System.Drawing.Point(20, 20)
$form.Controls.Add($titleLabel)

$versionLabel = New-Object System.Windows.Forms.Label
$versionLabel.Text = "v$Version par $DeveloperName"
$versionLabel.Font = New-Object System.Drawing.Font("Segoe UI", 10, [System.Drawing.FontStyle]::Regular)
$versionLabel.ForeColor = [System.Drawing.Color]::Gray
$versionLabel.AutoSize = $true
$versionLabel.Location = New-Object System.Drawing.Point(20, 50)
$form.Controls.Add($versionLabel)

$listPanel = New-Object System.Windows.Forms.Panel
$listPanel.AutoScroll = $true
$listPanel.Location = New-Object System.Drawing.Point(20, 80)
$listPanel.Size = New-Object System.Drawing.Size(760, 400)
$listPanel.BackColor = [System.Drawing.Color]::FromArgb(40, 40, 40)
$listPanel.BorderStyle = "FixedSingle"
$form.Controls.Add($listPanel)

$downloadProgress = New-Object System.Windows.Forms.ProgressBar
$downloadProgress.Location = New-Object System.Drawing.Point(20, 500)
$downloadProgress.Size = New-Object System.Drawing.Size(360, 20)
$downloadProgress.Style = "Continuous"
$downloadProgress.ForeColor = [System.Drawing.Color]::FromArgb(30, 144, 255)
$downloadProgress.BackColor = [System.Drawing.Color]::FromArgb(60, 60, 60)
$form.Controls.Add($downloadProgress)

$downloadLabel = New-Object System.Windows.Forms.Label
$downloadLabel.Text = "Téléchargement :"
$downloadLabel.ForeColor = [System.Drawing.Color]::White
$downloadLabel.Location = New-Object System.Drawing.Point(20, 480)
$downloadLabel.AutoSize = $true
$form.Controls.Add($downloadLabel)

$installProgress = New-Object System.Windows.Forms.ProgressBar
$installProgress.Location = New-Object System.Drawing.Point(400, 500)
$installProgress.Size = New-Object System.Drawing.Size(360, 20)
$installProgress.Style = "Continuous"
$installProgress.ForeColor = [System.Drawing.Color]::FromArgb(30, 144, 255)
$installProgress.BackColor = [System.Drawing.Color]::FromArgb(60, 60, 60)
$form.Controls.Add($installProgress)

$installLabel = New-Object System.Windows.Forms.Label
$installLabel.Text = "Installation :"
$installLabel.ForeColor = [System.Drawing.Color]::White
$installLabel.Location = New-Object System.Drawing.Point(400, 480)
$installLabel.AutoSize = $true
$form.Controls.Add($installLabel)

$resultBox = New-Object System.Windows.Forms.TextBox
$resultBox.Multiline = $true
$resultBox.Location = New-Object System.Drawing.Point(20, 530)
$resultBox.Size = New-Object System.Drawing.Size(760, 80)
$resultBox.ReadOnly = $true
$resultBox.BackColor = [System.Drawing.Color]::FromArgb(40, 40, 40)
$resultBox.ForeColor = [System.Drawing.Color]::White
$resultBox.BorderStyle = "FixedSingle"
$resultBox.Text = "Cliquez sur 'Vérifier' pour commencer."
$form.Controls.Add($resultBox)

$updateAllButton = New-Object System.Windows.Forms.Button
$updateAllButton.Text = "Tout Mettre à Jour"
$updateAllButton.Location = New-Object System.Drawing.Point(20, 620)
$updateAllButton.Size = New-Object System.Drawing.Size(180, 40)
$updateAllButton.BackColor = [System.Drawing.Color]::FromArgb(30, 144, 255)
$updateAllButton.ForeColor = [System.Drawing.Color]::White
$updateAllButton.FlatStyle = [System.Windows.Forms.FlatStyle]::Flat
$updateAllButton.FlatAppearance.BorderSize = 0
$updateAllButton.Enabled = $false
$updateAllButton.Add_Click({ Update-All })
$form.Controls.Add($updateAllButton)

$checkButton = New-Object System.Windows.Forms.Button
$checkButton.Text = "Vérifier les mises à jour"
$checkButton.Location = New-Object System.Drawing.Point(210, 620)
$checkButton.Size = New-Object System.Drawing.Size(180, 40)
$checkButton.BackColor = [System.Drawing.Color]::FromArgb(30, 144, 255)
$checkButton.ForeColor = [System.Drawing.Color]::White
$checkButton.FlatStyle = [System.Windows.Forms.FlatStyle]::Flat
$checkButton.FlatAppearance.BorderSize = 0
$checkButton.Add_Click({ Check-Updates })
$form.Controls.Add($checkButton)

$logButton = New-Object System.Windows.Forms.Button
$logButton.Text = "Voir les Logs"
$logButton.Location = New-Object System.Drawing.Point(400, 620)
$logButton.Size = New-Object System.Drawing.Size(180, 40)
$logButton.BackColor = [System.Drawing.Color]::FromArgb(100, 100, 100)
$logButton.ForeColor = [System.Drawing.Color]::White
$logButton.FlatStyle = [System.Windows.Forms.FlatStyle]::Flat
$logButton.FlatAppearance.BorderSize = 0
$logButton.Add_Click({ 
    if (Test-Path $logPath) {
        Start-Process notepad $logPath
    } else {
        [System.Windows.Forms.MessageBox]::Show("Le fichier de log n'existe pas.", "Erreur", [System.Windows.Forms.MessageBoxButtons]::OK, [System.Windows.Forms.MessageBoxIcon]::Warning)
    }
})
$form.Controls.Add($logButton)

$errorButton = New-Object System.Windows.Forms.Button
$errorButton.Text = "Voir les Erreurs"
$errorButton.Location = New-Object System.Drawing.Point(590, 620)
$errorButton.Size = New-Object System.Drawing.Size(180, 40)
$errorButton.BackColor = [System.Drawing.Color]::FromArgb(100, 100, 100)
$errorButton.ForeColor = [System.Drawing.Color]::White
$errorButton.FlatStyle = [System.Windows.Forms.FlatStyle]::Flat
$errorButton.FlatAppearance.BorderSize = 0
$errorButton.Add_Click({ 
    if (Test-Path $errorLogPath) {
        Start-Process notepad $errorLogPath
    } else {
        [System.Windows.Forms.MessageBox]::Show("Le fichier des erreurs n'existe pas.", "Erreur", [System.Windows.Forms.MessageBoxButtons]::OK, [System.Windows.Forms.MessageBoxIcon]::Warning)
    }
})
$form.Controls.Add($errorButton)

# --- Demander l'élévation si non administrateur ---
if (-not (Test-Admin)) {
    $resultBox.Text = "Avertissement : Exécutez l'application en mode administrateur pour garantir toutes les mises à jour."
    Write-Log "Avertissement : Droits administrateur non détectés au démarrage." $errorLogPath
    $elevateButton = New-Object System.Windows.Forms.Button
    $elevateButton.Text = "Relancer en Admin"
    $elevateButton.Location = New-Object System.Drawing.Point(600, 20)
    $elevateButton.Size = New-Object System.Drawing.Size(120, 30)
    $elevateButton.BackColor = [System.Drawing.Color]::FromArgb(255, 69, 0)
    $elevateButton.ForeColor = [System.Drawing.Color]::White
    $elevateButton.FlatStyle = [System.Windows.Forms.FlatStyle]::Flat
    $elevateButton.FlatAppearance.BorderSize = 0
    $elevateButton.Add_Click({
        try {
            if (-not (Test-Path $exePath)) {
                Write-Log "Erreur : Le fichier $exePath n'existe pas." $errorLogPath
                [System.Windows.Forms.MessageBox]::Show("Erreur : Le fichier $exePath n'existe pas.", "Erreur", [System.Windows.Forms.MessageBoxButtons]::OK, [System.Windows.Forms.MessageBoxIcon]::Error)
                return
            }
            Start-Process -FilePath $exePath -Verb RunAs
            $form.Close()
        } catch {
            Write-Log "Erreur lors de la tentative de relance en mode administrateur : $_" $errorLogPath
            [System.Windows.Forms.MessageBox]::Show("Erreur lors de la tentative de relance en mode administrateur : $_", "Erreur", [System.Windows.Forms.MessageBoxButtons]::OK, [System.Windows.Forms.MessageBoxIcon]::Error)
        }
    })
    $form.Controls.Add($elevateButton)
}

# --- Exécution ---
$wingetResult, $null = Test-Winget
if ($wingetResult -and (Test-Network)) {
    Check-Updates
} else {
    $resultBox.Text = "Erreur : Winget ou la connexion réseau n'est pas disponible. Vérifiez votre installation et connexion."
}
$form.ShowDialog() | Out-Null