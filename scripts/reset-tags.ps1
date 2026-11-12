# 清空所有 tag（本地 + 远程），然后在当前提交上重新打一个 tag。
#
# 用法（在有 GitHub 访问权限的机器上执行）：
#   pwsh scripts/reset-tags.ps1 -Tag v1.0.0-rc
#   pwsh scripts/reset-tags.ps1 -Tag v1.0.0-rc -PushBranch      # 同时推送当前分支
#   pwsh scripts/reset-tags.ps1 -Tag v1.0.0-rc -DryRun          # 只看会删什么，不真删
#
# 说明：远程 tag 是存在 GitHub 上的，必须联网用 `git push --delete` 删；
#       本脚本会先本地删、再远程删，最后按 -Tag 打新 tag 并推送。

param(
    [Parameter(Mandatory = $true)][string]$Tag,
    [string]$Remote = "origin",
    [switch]$PushBranch,
    [switch]$DryRun
)

$ErrorActionPreference = "Stop"

Write-Host "=== 1. 本地 tag ===" -ForegroundColor Cyan
$localTags = @(git tag -l)
if ($localTags.Count -eq 0) {
    Write-Host "（本地没有 tag）"
} else {
    foreach ($t in $localTags) {
        if ($DryRun) { Write-Host "[dry-run] git tag -d $t" }
        else { git tag -d $t | Out-Null; Write-Host "已删除本地 tag: $t" }
    }
}

Write-Host "=== 2. 远程 tag（$Remote）===" -ForegroundColor Cyan
$remoteTags = @()
try {
    $remoteTags = @(git ls-remote --tags --refs $Remote |
        ForEach-Object { ($_ -split "`t")[1] -replace '^refs/tags/', '' } |
        Where-Object { $_ })
} catch {
    Write-Warning "读取远程 tag 失败（网络不通？）：$($_.Exception.Message)"
}

if ($remoteTags.Count -eq 0) {
    Write-Host "（远程没有 tag，或读取失败）"
} else {
    foreach ($t in $remoteTags) {
        if ($DryRun) { Write-Host "[dry-run] git push $Remote --delete $t" }
        else {
            git push $Remote --delete $t
            Write-Host "已删除远程 tag: $t"
        }
    }
}

if ($PushBranch) {
    Write-Host "=== 3. 推送当前分支 ===" -ForegroundColor Cyan
    $branch = (git rev-parse --abbrev-ref HEAD).Trim()
    if ($DryRun) { Write-Host "[dry-run] git push $Remote $branch" }
    else { git push $Remote $branch }
}

Write-Host "=== 4. 新建 tag $Tag ===" -ForegroundColor Cyan
if ($DryRun) {
    Write-Host "[dry-run] git tag -a $Tag -m ""$Tag"" ; git push $Remote $Tag"
} else {
    $exists = @(git tag -l $Tag)
    if ($exists.Count -gt 0) { git tag -d $Tag | Out-Null }
    git tag -a $Tag -m "$Tag"
    git push $Remote $Tag
    Write-Host "已创建并推送 tag: $Tag" -ForegroundColor Green
}

Write-Host ""
Write-Host "完成。远程 tag 现状：" -ForegroundColor Cyan
git ls-remote --tags --refs $Remote
