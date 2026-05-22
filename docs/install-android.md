# Installing Voxply on Android

Voxply for Android is distributed as a sideloaded APK — there is no Play
Store listing. You install it directly from `releases.voxply.io`.

## Requirements

- Android 7.0 (API 24) or newer
- A Chromium-based Android System WebView (version 100+). This ships by
  default on all modern Android devices; de-Googled ROMs that pin an old
  WebView may need to update it manually.

## Steps

1. On your Android device, open **Settings → Apps → Special app access →
   Install unknown apps**.
2. Find your browser (e.g. Chrome) and enable **Allow from this source**.
3. Open `releases.voxply.io` in your browser and tap the latest
   `voxply-android.apk` link to download it.
4. When the download finishes, tap **Open** (or find it in your Downloads
   folder) and tap **Install**.
5. Android may show a **Play Protect** warning — tap **Install anyway**.
   This warning appears because the APK is not distributed through the
   Play Store, not because it contains anything harmful.

## Updating

The app checks for updates at startup and shows a banner when a new version
is available. Tap **Download** in the banner, then follow steps 3–5 above.
Installing a newer release-signed APK over an existing one preserves all
your data and settings.

## Uninstalling

Standard Android uninstall (Settings → Apps → Voxply → Uninstall). Your
identity seed is stored in the Android Keystore and is deleted with the app.
Make sure you have your **24-word recovery phrase** saved before uninstalling
if you want to recover your identity on another device.
