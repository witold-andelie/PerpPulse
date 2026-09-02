param(
  [string]$ProjectId = "project-5e761e8c-65aa-4033-8cb",
  [string]$Region = "europe-west3",
  [string]$BillingAccount = "01B820-8960C8-EFE153",
  [string]$BudgetAmount = "350",
  [switch]$SkipSql,
  [switch]$SkipBudget
)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot

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

function Test-Gcloud {
  param([Parameter(ValueFromRemainingArguments = $true)][string[]]$Arguments)
  $previousPreference = $ErrorActionPreference
  try {
    $ErrorActionPreference = "Continue"
    & gcloud.cmd @Arguments *> $null
    return $LASTEXITCODE -eq 0
  }
  finally {
    $ErrorActionPreference = $previousPreference
  }
}

$repository = "perppulse"
$runtimeAccountId = "perppulse-runtime"
$runtimeAccount = "$runtimeAccountId@$ProjectId.iam.gserviceaccount.com"
$bucket = "$ProjectId-perppulse-evidence"
$sqlInstance = "perppulse-pg"
$database = "perppulse"
$dbUser = "perppulse"
$secretId = "perppulse-db-password"
$budgetTopic = "perppulse-budget"
$labels = "project=perppulse,environment=hackathon,owner=witold-andelie,expiry=2026-10-15"

Write-Output "Project=$ProjectId"
Write-Output "Region=$Region"
Write-Output "BillingAccount=$BillingAccount"
Write-Output "Creating always-on Cloud SQL and supporting identities. Cloud Run worker/API images are not deployed yet."

Invoke-Gcloud config set project $ProjectId
Invoke-Gcloud config set compute/region $Region

$apis = @(
  "run.googleapis.com",
  "sqladmin.googleapis.com",
  "storage.googleapis.com",
  "artifactregistry.googleapis.com",
  "cloudbuild.googleapis.com",
  "secretmanager.googleapis.com",
  "logging.googleapis.com",
  "monitoring.googleapis.com",
  "iam.googleapis.com",
  "cloudresourcemanager.googleapis.com",
  "pubsub.googleapis.com",
  "cloudscheduler.googleapis.com",
  "billingbudgets.googleapis.com",
  "servicenetworking.googleapis.com"
)
Invoke-Gcloud services enable @apis

if (-not (Test-Gcloud artifacts repositories describe $repository --location $Region)) {
  Invoke-Gcloud artifacts repositories create $repository `
    --repository-format docker `
    --location $Region `
    --description "PerpPulse container images" `
    --labels $labels
}

if (-not (Test-Gcloud iam service-accounts describe $runtimeAccount)) {
  Invoke-Gcloud iam service-accounts create $runtimeAccountId `
    --display-name "PerpPulse runtime" `
    --description "Least-privilege runtime for PerpPulse Cloud Run and jobs"
}

$projectNumber = ([string](Invoke-Gcloud projects describe $ProjectId --format "value(projectNumber)")).Trim()

if (-not (Test-Gcloud storage buckets describe "gs://$bucket")) {
  Invoke-Gcloud storage buckets create "gs://$bucket" `
    --project $ProjectId `
    --location $Region `
    --uniform-bucket-level-access
}
Invoke-Gcloud storage buckets update "gs://$bucket" --lifecycle-file (Join-Path $root "deploy\gcs-lifecycle.json")
Invoke-Gcloud storage buckets update "gs://$bucket" --update-labels $labels
Invoke-Gcloud storage buckets add-iam-policy-binding "gs://$bucket" `
  --member "serviceAccount:$runtimeAccount" `
  --role roles/storage.objectAdmin

foreach ($role in @("roles/cloudsql.client", "roles/secretmanager.secretAccessor", "roles/logging.logWriter", "roles/monitoring.metricWriter")) {
  Invoke-Gcloud projects add-iam-policy-binding $ProjectId `
    --member "serviceAccount:$runtimeAccount" `
    --role $role `
    --condition=None
}

Invoke-Gcloud artifacts repositories add-iam-policy-binding $repository `
  --location $Region `
  --member "serviceAccount:$runtimeAccount" `
  --role roles/artifactregistry.reader

if (-not $SkipBudget) {
  if (-not (Test-Gcloud pubsub topics describe $budgetTopic)) {
    Invoke-Gcloud pubsub topics create $budgetTopic
  }
  $previousPreference = $ErrorActionPreference
  $ErrorActionPreference = "Continue"
  & gcloud.cmd pubsub topics add-iam-policy-binding $budgetTopic `
    --member "serviceAccount:cloud-billing@system.gserviceaccount.com" `
    --role roles/pubsub.publisher *> $null
  $budgetNames = & gcloud.cmd billing budgets list --billing-account $BillingAccount --format "value(displayName)" 2>$null
  $ErrorActionPreference = $previousPreference
  if (($budgetNames -notcontains "PerpPulse350") -and ($budgetNames -notcontains "PerpPulse400") -and ($budgetNames -notcontains "PerpPulse USD 400 ceiling")) {
    $ErrorActionPreference = "Continue"
    & gcloud.cmd billing budgets create `
      --billing-account $BillingAccount `
      --display-name PerpPulse350 `
      --budget-amount $BudgetAmount `
      --filter-projects "projects/$ProjectId" `
      --threshold-rule percent=0.25 `
      --threshold-rule percent=0.50 `
      --threshold-rule percent=0.75 `
      --threshold-rule percent=0.90 `
      --threshold-rule percent=1.0
    $budgetExit = $LASTEXITCODE
    $ErrorActionPreference = $previousPreference
    if ($budgetExit -ne 0) {
      Write-Output "WARNING: billing budget create failed with exit $budgetExit. Continue provisioning. Create the EUR 350 budget in the console if needed."
    }
  }
}

if (-not $SkipSql) {
  if (-not (Test-Gcloud sql instances describe $sqlInstance)) {
    Invoke-Gcloud sql instances create $sqlInstance `
      --database-version POSTGRES_16 `
      --edition enterprise `
      --tier db-g1-small `
      --region $Region `
      --availability-type ZONAL `
      --storage-size 20 `
      --storage-type SSD `
      --storage-auto-increase `
      --backup-start-time 03:00 `
      --retained-backups-count 7 `
      --maintenance-window-day SUN `
      --maintenance-window-hour 4 `
      --assign-ip `
      --no-deletion-protection `
      --storage-auto-increase-limit 40
  }

  if (-not (Test-Gcloud sql databases describe $database --instance $sqlInstance)) {
    Invoke-Gcloud sql databases create $database --instance $sqlInstance
  }

  $passwordFile = Join-Path $env:TEMP "perppulse-db-password.txt"
  try {
    if (-not (Test-Gcloud secrets describe $secretId)) {
      $bytes = New-Object byte[] 32
      [System.Security.Cryptography.RandomNumberGenerator]::Create().GetBytes($bytes)
      $password = [Convert]::ToBase64String($bytes).Replace("+", "-").Replace("/", "_").TrimEnd("=")
      Set-Content -LiteralPath $passwordFile -Value $password -NoNewline -Encoding ascii
      Invoke-Gcloud secrets create $secretId --data-file $passwordFile --replication-policy user-managed --locations $Region
      Invoke-Gcloud sql users create $dbUser --instance $sqlInstance --password $password
    }
    else {
      $userExists = [string](& gcloud.cmd sql users list --instance $sqlInstance --filter "name=$dbUser" --format "value(name)" 2>$null)
      if (-not $userExists) {
        Invoke-Gcloud secrets versions access latest --secret $secretId --out-file $passwordFile
        $password = [System.IO.File]::ReadAllText($passwordFile)
        Invoke-Gcloud sql users create $dbUser --instance $sqlInstance --password $password
      }
    }
  }
  finally {
    if (Test-Path $passwordFile) {
      Remove-Item -LiteralPath $passwordFile -Force
    }
  }

  Invoke-Gcloud secrets add-iam-policy-binding $secretId `
    --member "serviceAccount:$runtimeAccount" `
    --role roles/secretmanager.secretAccessor
}

Write-Output "Foundation complete."
Write-Output "ArtifactRegistry=$Region-docker.pkg.dev/$ProjectId/$repository"
Write-Output "Bucket=gs://$bucket"
Write-Output "RuntimeServiceAccount=$runtimeAccount"
Write-Output "CloudSqlInstance=$sqlInstance"
Write-Output "Database=$database"
Write-Output "DatabaseUser=$dbUser"
Write-Output "DatabasePasswordSecret=$secretId"
Write-Output "Cloud Run worker and API were not deployed; there is no container image yet."
Write-Output "Stop Cloud SQL later with: gcloud sql instances patch $sqlInstance --activation-policy=NEVER --project $ProjectId"
