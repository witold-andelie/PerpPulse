# Envio mainnet operating recording

[Download the 80-second WebM recording](perppulse-envio-live.webm).
[English SRT](perppulse-envio-live.srt) and [WebVTT](perppulse-envio-live.vtt)
captions are included. The recording has no audio. It contains actual browser
output; financial values were not substituted or generated for the video.

To watch with captions from the repository root:

```powershell
py -3 -m http.server 18083 --bind 127.0.0.1 --directory docs/demo
```

Open `http://127.0.0.1:18083/`. The standalone player has no external scripts or
analytics. This command serves the archived recording, not the live application.
The public GitHub source page also permits downloading the WebM and captions.
Submission-portal codec acceptance and a hosted player URL remain to be checked.

The flow is Envio-indexed Monad events, coverage-qualified account replay,
position evidence, one-cutoff manifest export, and refresh with advancing
coverage. The initial selected cutoff was block 110018676; refresh reached
110018887. Accounts 5382-5385 were created within the indexed range.

[Recording provenance](recording.json) includes its duration, dimensions,
SHA-256, observations, and visual checks. The [downloaded manifest](manifest.json)
belongs to the initial immutable UI snapshot. The independent
[position diagnostic](../evidence/sdk-position-diagnostic-2026-10-02.json)
uses a different observation and must not be attached to that snapshot as a
matching reconciliation scorecard.

This is a bounded self-hosted mainnet demonstration. Continuous public hosting,
global analytics, eligible accounting marks, full SDK reconciliation and live
Nansen enrichment remain pending. No order placement or custody is included.
