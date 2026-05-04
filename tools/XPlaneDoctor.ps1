Add-Type -AssemblyName System.Windows.Forms

$projectRoot = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
$cargo = Join-Path $env:USERPROFILE ".cargo\bin\cargo.exe"

if (-not (Test-Path $cargo)) {
    [System.Windows.Forms.MessageBox]::Show("cargo.exe not found: $cargo", "X-Plane Log Triage Tool")
    exit 1
}

$message = "Choose Yes to analyze a Log.txt file. Choose No to scan an X-Plane folder."
$title = "X-Plane Log Triage Tool"
$buttons = [System.Windows.Forms.MessageBoxButtons]::YesNoCancel
$choice = [System.Windows.Forms.MessageBox]::Show($message, $title, $buttons)

if ($choice -eq [System.Windows.Forms.DialogResult]::Cancel) {
    exit 0
}

if ($choice -eq [System.Windows.Forms.DialogResult]::Yes) {
    $dialog = New-Object System.Windows.Forms.OpenFileDialog
    $dialog.Title = "Choose X-Plane Log.txt"
    $dialog.Filter = "X-Plane Log file (Log.txt)|Log.txt|Text files (*.txt)|*.txt|All files (*.*)|*.*"
    if ($dialog.ShowDialog() -ne [System.Windows.Forms.DialogResult]::OK) {
        exit 0
    }

    Push-Location $projectRoot
    & $cargo run --bin xplane-doctor -- analyze-log $dialog.FileName
    Pop-Location
} else {
    $dialog = New-Object System.Windows.Forms.FolderBrowserDialog
    $dialog.Description = "Choose X-Plane 12 root folder"
    if ($dialog.ShowDialog() -ne [System.Windows.Forms.DialogResult]::OK) {
        exit 0
    }

    Push-Location $projectRoot
    & $cargo run --bin xplane-doctor -- scan $dialog.SelectedPath
    Pop-Location
}

$report = Join-Path ([Environment]::GetFolderPath("MyDocuments")) "X-Plane Log Triage Reports\report.html"
if (Test-Path $report) {
    Start-Process $report
} else {
    [System.Windows.Forms.MessageBox]::Show("Analysis finished, but report was not found: $report", "X-Plane Log Triage Tool")
}
