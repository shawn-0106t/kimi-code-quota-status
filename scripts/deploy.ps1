﻿# quota-status 部署脚本（P1 自动化）：构建 -> 部署 -> sha256 校验。
# 用法：powershell -File scripts/deploy.ps1        （Windows PowerShell 5.1 可用）
#       pwsh -File scripts/deploy.ps1 [-DstDir C:\tools]   （pwsh 7 亦可）
# 默认部署位 ~/.kimi-code/bin（维护者布局）；README「安装」示例布局（如 C:\tools）
# 可传 -DstDir 覆盖。对应 docs/HANDOFF.md §5 手工流程："重新构建后需重新复制到
# 该路径，文件被运行中会话占用时，先把旧 exe 改名为 quota-status.exe.old 再复制新版"。

param(
    # 部署目录；默认维护者布局，README 示例布局经参数覆盖
    [string]$DstDir = (Join-Path $HOME ".kimi-code\bin")
)

$ErrorActionPreference = "Stop"

$repoRoot = Split-Path -Parent $PSScriptRoot
$src = Join-Path $repoRoot "target\release\quota-status.exe"
$dst = Join-Path $DstDir "quota-status.exe"
$old = Join-Path $DstDir "quota-status.exe.old"

# 1. 构建（--locked 与 CI 一致）。PS 5.1 陷阱（review m-2 实测确认）：EAP=Stop 时
#    native 命令的 stderr 行（cargo 进度输出走 stderr）一经 2>&1 重定向即抛
#    NativeCommandError，ForEach-Object 字符串化也拦不住——正确做法是局部把
#    EAP 降为 Continue 让 stderr 行流经管道，成败只看 $LASTEXITCODE
Push-Location $repoRoot
try {
    $prevEap = $ErrorActionPreference
    $ErrorActionPreference = "Continue"
    try {
        cargo build --release --locked 2>&1 | ForEach-Object { "$_" }
    } finally {
        $ErrorActionPreference = $prevEap
    }
    if ($LASTEXITCODE -ne 0) { throw "cargo build --release 失败，中止部署" }
} finally {
    Pop-Location
}

# 2. 确保部署目录存在
New-Item -ItemType Directory -Force -Path $DstDir | Out-Null

# 3. 复制；失败时的兜底护栏（review M-1）：
#    - 部署位无旧 exe（首次部署）-> 直接失败，无从兜底；
#    - 源产物缺失（构建产物异常）-> 不触碰部署位；
#    - 旧 exe 被占用（真实兜底场景）-> 改名 .old 后重试复制；
#    - 兜底复制仍失败 -> 回滚 .old -> dst，部署位绝不能落空（statusline 不能断）
try {
    Copy-Item $src $dst -Force
} catch {
    if (-not (Test-Path $dst)) {
        throw "复制失败且部署位无旧 exe（首次部署场景），无法兜底：$_"
    }
    if (-not (Test-Path $src)) {
        throw "复制失败且源产物缺失（构建产物异常），不触碰部署位：$_"
    }
    Write-Host "直接复制失败（旧 exe 可能被运行中会话占用），改用改名兜底..."
    $bak = $old
    if (Test-Path $old) {
        try {
            Remove-Item $old -Force
        } catch {
            # 历史 .old 自身被锁（罕见）：改用带时间戳的备份名，不中止部署
            $bak = "$old.$(Get-Date -Format yyyyMMdd-HHmmss)"
        }
    }
    Move-Item $dst $bak -Force
    try {
        Copy-Item $src $dst -Force
    } catch {
        Move-Item $bak $dst -Force
        throw "兜底复制仍失败，已回滚旧 exe 保持部署位可用；根因：$_"
    }
}

# 4. sha256 校验：源与部署位一致才算成功
$srcHash = (Get-FileHash $src -Algorithm SHA256).Hash
$dstHash = (Get-FileHash $dst -Algorithm SHA256).Hash
if ($srcHash -ne $dstHash) { throw "sha256 不一致，部署未生效：src=$srcHash dst=$dstHash" }

Write-Host "部署完成: $dst"
Write-Host "sha256 : $dstHash"
