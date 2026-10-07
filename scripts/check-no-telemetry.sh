#!/usr/bin/env bash
# Zafe sends no telemetry: no analytics, crash-reporting or tracking SDK may enter the
# dependency files. Firebase Messaging (content-free push) is the one Firebase piece we
# allow. A deliberate exception needs an edit here, reviewed with the privacy docs.
set -euo pipefail
cd "$(dirname "$0")/.."

banned='sentry|crashlytics|firebase[_-]?analytics|firebase[_-]?crash|firebase[_-]?performance|google[_-]?analytics|google[_-]?mobile[_-]?ads|play-services-measurement|posthog|mixpanel|amplitude|segment|appsflyer|adjust[_-]?sdk|branch[_-]?sdk|datadog|bugsnag|instabug|smartlook|matomo|plausible|countly|heap[_-]?analytics|appcenter|new[_-]?relic|embrace|raygun|rollbar|logrocket|clarity|opentelemetry|tracing[_-]?opentelemetry|sentry-rust'

files=$(git ls-files 'app/pubspec.yaml' 'app/pubspec.lock' 'app/android/**/*.gradle' \
  'app/android/**/*.gradle.kts' 'app/android/**/*.toml' 'app/ios/Podfile' 'app/ios/Podfile.lock' \
  'app/ios/**/Package.resolved' 'Cargo.toml' '**/Cargo.toml' 'Cargo.lock' 'infra/site/package.json' \
  'infra/site/package-lock.json' 2>/dev/null || true)

status=0
for f in $files; do
  # Whole package names, not substrings (unicode-segmentation is not "segment").
  if hits=$(grep -vE '^[[:space:]]*(#|//)' "$f" | grep -oE '[A-Za-z0-9_.@/:-]+' \
      | grep -Ei "^($banned)($|[-_./:])" | sort -u); then
    echo "telemetry dependency in $f:"; echo "$hits"; status=1
  fi
done
# Android manifest: no analytics/ads IDs or collection switches left on.
if grep -rnEi 'firebase_analytics_collection|google_analytics|AD_ID|com\.google\.android\.gms\.ads' \
    app/android/app/src --include=AndroidManifest.xml 2>/dev/null; then status=1; fi
[ $status -eq 0 ] && echo "no telemetry dependencies"
exit $status
