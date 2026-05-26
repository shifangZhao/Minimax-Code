# Git operations for Minimax Code
$ErrorActionPreference = 'Continue'
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8

Set-Location 'E:\NEWproject\Minimax Code'
Write-Host "Current directory: $(Get-Location)"
Write-Host "--- Git Status ---"
git status
Write-Host "--- Git Add ---"
git add -A
Write-Host "--- Git Commit ---"
git commit -m "Update: add BackgroundTaskPanel, useBackgroundTasks, utils, and various improvements"
Write-Host "--- Git Push ---"
git push -u origin master