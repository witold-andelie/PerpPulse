param(
  [string]$ProjectId = "project-5e761e8c-65aa-4033-8cb",
  [string]$SqlInstance = "perppulse-pg"
)

$ErrorActionPreference = "Stop"

function Invoke-Gcloud {
  param([Parameter(ValueFromRemainingArguments = $true)][string[]]$Arguments)
  $previousPreference = $ErrorActionPreference
  try {
    $ErrorActionPreference = "Continue"
    & gcloud.cmd @Arguments
    if ($LASTEXITCODE -ne 0) {
      throw "gcloud command failed with exit code $LASTEXITCODE : $Arguments"
    }
  }
  finally {
    $ErrorActionPreference = $previousPreference
  }
}

Invoke-Gcloud config set project $ProjectId
Invoke-Gcloud sql instances patch $SqlInstance --activation-policy=NEVER
Write-Output "Cloud SQL instance $SqlInstance is set to NEVER (stopped). Data is retained; compute charges pause."
Write-Output "Restart with: gcloud sql instances patch $SqlInstance --activation-policy=ALWAYS --project $ProjectId"
