param(
  [Parameter(Mandatory = $true)]
  [string]$ImageUri,
  [switch]$Execute
)

$ErrorActionPreference = 'Stop'
$project = 'project-5e761e8c-65aa-4033-8cb'
$region = 'europe-west3'
$billing = '01B820-8960C8-EFE153'
$imagePrefix = "$region-docker.pkg.dev/$project/perppulse/"
if (-not $ImageUri.StartsWith($imagePrefix) -or $ImageUri -notmatch '@sha256:[0-9a-f]{64}$') {
  throw 'Use an immutable image digest in the owner-confirmed PerpPulse Artifact Registry.'
}

$deployArguments = @(
  'run', 'deploy', 'perppulse-demo', '--project', $project, '--region', $region,
  '--image', $ImageUri, '--service-account', "perppulse-runtime@$project.iam.gserviceaccount.com",
  '--port', '8080', '--min-instances', '0', '--max-instances', '1',
  '--cpu', '1', '--memory', '512Mi', '--concurrency', '16', '--timeout', '30',
  '--no-allow-unauthenticated', '--labels', 'app=perppulse,environment=demo'
)
Write-Output "Target: $project / $region / billing $billing / ceiling EUR 350"
Write-Output 'Mode: synthetic fixture smoke deployment. This does not establish mainnet operation.'
Write-Output ('gcloud ' + ($deployArguments -join ' '))
if (-not $Execute) {
  Write-Output 'Dry run only. Execute requires explicit owner approval for this billable deployment.'
  return
}
$currentBilling = & gcloud.cmd billing projects describe $project --format='value(billingAccountName,billingEnabled)'
if ($LASTEXITCODE -ne 0 -or $currentBilling -notmatch [regex]::Escape($billing) -or $currentBilling -notmatch 'True') {
  throw 'Project billing does not match the owner-confirmed account or is not enabled.'
}
& gcloud.cmd @deployArguments
if ($LASTEXITCODE -ne 0) { throw 'Cloud Run deployment failed.' }
