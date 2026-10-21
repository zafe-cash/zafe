package xyz.zafe.zafe

import android.app.KeyguardManager
import android.app.UiModeManager
import android.content.ActivityNotFoundException
import android.content.ClipData
import android.content.ClipDescription
import android.content.ClipboardManager
import android.content.Intent
import android.media.AudioAttributes
import android.media.AudioManager
import android.media.SoundPool
import android.os.Build
import android.os.Bundle
import android.os.PersistableBundle
import android.provider.Settings
import android.view.HapticFeedbackConstants
import android.view.WindowManager
import androidx.activity.enableEdgeToEdge
import androidx.biometric.BiometricManager.Authenticators
import androidx.biometric.BiometricPrompt
import androidx.core.content.ContextCompat
import io.flutter.embedding.android.FlutterFragmentActivity
import io.flutter.embedding.engine.FlutterEngine
import io.flutter.plugin.common.MethodChannel

// A FragmentActivity so local_auth can show the biometric / screen-lock prompt.
class MainActivity : FlutterFragmentActivity() {
    /// A secret screen asked to block capture (`xyz.zafe/secure_screen`).
    private var secureRequested = false

    /// The payment sounds (`xyz.zafe/payment_feedback`), loaded once per activity.
    private var soundPool: SoundPool? = null
    private val soundIds = mutableMapOf<String, Int>()
    private val loadedSounds = mutableSetOf<Int>()

    /// The unlock waiting for the PIN screen's answer (Android 10 and older).
    private var pendingUnlock: MethodChannel.Result? = null

    override fun onCreate(savedInstanceState: Bundle?) {
        // Edge to edge on every Android version, not only 15+ (where targetSdk 35+ forces
        // it): one layout everywhere, Flutter's padding handles the bars (Play's
        // recommendation, 2026-10-22).
        enableEdgeToEdge()
        super.onCreate(savedInstanceState)
        // Taps that arrive while another app draws over this one (an overlay trying to
        // trick the owner into approving) are dropped.
        window.decorView.filterTouchesWhenObscured = true
        // A wallet's balances and payments stay out of the recent-apps view (the card
        // shows a blank window instead). Screenshots inside the app still work.
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
            setRecentsScreenshotEnabled(false)
        }
    }

    // Before Android 13 there is no switch for the thumbnail alone: block capture while
    // the app is in the background, which also blanks the thumbnail taken as it leaves.
    override fun onPause() {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.TIRAMISU) {
            window.addFlags(WindowManager.LayoutParams.FLAG_SECURE)
        }
        super.onPause()
    }

    override fun onResume() {
        super.onResume()
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.TIRAMISU && !secureRequested) {
            window.clearFlags(WindowManager.LayoutParams.FLAG_SECURE)
        }
    }

    override fun onDestroy() {
        soundPool?.release()
        soundPool = null
        super.onDestroy()
    }

    /// Haptic taps per payment moment, in ms: the same tempo as the sound's strikes
    /// (scripts/sounds/sounds.py), like Apple Pay's two taps under its two notes.
    private val paymentTaps = mapOf(
        "approve" to longArrayOf(0),
        "ready" to longArrayOf(0, 120),
        "sent" to longArrayOf(0, 130, 260),
        "received" to longArrayOf(0, 110),
        "failed" to longArrayOf(0, 120),
    )

    private fun loadPaymentSounds() {
        if (soundPool != null) return
        val pool = SoundPool.Builder()
            .setMaxStreams(2)
            .setAudioAttributes(
                AudioAttributes.Builder()
                    // Sonification: system-sound volume, silenced with the ringer.
                    .setUsage(AudioAttributes.USAGE_ASSISTANCE_SONIFICATION)
                    .setContentType(AudioAttributes.CONTENT_TYPE_SONIFICATION)
                    .build(),
            )
            .build()
        pool.setOnLoadCompleteListener { _, id, status ->
            if (status == 0) loadedSounds.add(id)
        }
        mapOf(
            "approve" to R.raw.pay_approve,
            "ready" to R.raw.pay_ready,
            "sent" to R.raw.pay_sent,
            "received" to R.raw.pay_received,
            "failed" to R.raw.pay_failed,
        ).forEach { (moment, res) -> soundIds[moment] = pool.load(this, res, 1) }
        soundPool = pool
    }

    /// Plays a payment moment: the sound unless it's off or the phone is on silent or
    /// vibrate, and the haptic taps (which follow the system's touch-feedback setting).
    private fun playPayment(moment: String, sound: Boolean): Boolean {
        val taps = paymentTaps[moment] ?: return false
        val ringer = getSystemService(AudioManager::class.java)?.ringerMode
        val id = soundIds[moment]
        if (sound && ringer == AudioManager.RINGER_MODE_NORMAL && id != null && id in loadedSounds) {
            soundPool?.play(id, 1f, 1f, 1, 0, 1f)
        }
        val view = window.decorView
        val tap = when {
            moment == "failed" && Build.VERSION.SDK_INT >= Build.VERSION_CODES.R ->
                HapticFeedbackConstants.REJECT
            Build.VERSION.SDK_INT >= Build.VERSION_CODES.R -> HapticFeedbackConstants.CONFIRM
            else -> HapticFeedbackConstants.VIRTUAL_KEY
        }
        // REJECT already pulses twice: one is enough for a failure.
        val times = if (tap == HapticFeedbackConstants.REJECT) taps.take(1) else taps.toList()
        times.forEach { at -> view.postDelayed({ view.performHapticFeedback(tap) }, at) }
        return true
    }

    /// Asks for a strong biometric (class 3: fingerprint, secure face unlock) or the
    /// device credential. Weak biometrics, such as camera face unlock a photo can fool,
    /// are never offered. Answers `null` when unlocked, else an error code the Dart side
    /// maps like local_auth's (`userCanceled`, `biometricLockout`, ...).
    private fun unlock(reason: String, result: MethodChannel.Result) {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
            val prompt = BiometricPrompt(
                this,
                ContextCompat.getMainExecutor(this),
                object : BiometricPrompt.AuthenticationCallback() {
                    override fun onAuthenticationSucceeded(r: BiometricPrompt.AuthenticationResult) {
                        result.success(null)
                    }

                    override fun onAuthenticationError(code: Int, message: CharSequence) {
                        result.success(unlockError(code))
                    }
                },
            )
            val info = BiometricPrompt.PromptInfo.Builder()
                .setTitle("Unlock Zafe")
                .setSubtitle(reason)
                .setAllowedAuthenticators(
                    Authenticators.BIOMETRIC_STRONG or Authenticators.DEVICE_CREDENTIAL,
                )
                .build()
            prompt.authenticate(info)
        } else {
            // Before Android 11 the system can't combine strong biometrics with the
            // credential in one prompt: ask for the PIN, pattern or password alone.
            val intent = getSystemService(KeyguardManager::class.java)
                ?.createConfirmDeviceCredentialIntent("Unlock Zafe", reason)
            if (intent == null) {
                result.success("noCredentialsSet")
                return
            }
            pendingUnlock?.success("authInProgress")
            pendingUnlock = result
            @Suppress("DEPRECATION")
            startActivityForResult(intent, UNLOCK_REQUEST)
        }
    }

    private fun unlockError(code: Int): String = when (code) {
        BiometricPrompt.ERROR_USER_CANCELED,
        BiometricPrompt.ERROR_NEGATIVE_BUTTON -> "userCanceled"
        BiometricPrompt.ERROR_CANCELED -> "systemCanceled"
        BiometricPrompt.ERROR_TIMEOUT -> "timeout"
        BiometricPrompt.ERROR_LOCKOUT -> "temporaryLockout"
        BiometricPrompt.ERROR_LOCKOUT_PERMANENT -> "biometricLockout"
        BiometricPrompt.ERROR_NO_DEVICE_CREDENTIAL -> "noCredentialsSet"
        else -> "unknownError"
    }

    @Deprecated("Only the PIN screen of Android 10 and older answers here")
    override fun onActivityResult(requestCode: Int, resultCode: Int, data: Intent?) {
        if (requestCode == UNLOCK_REQUEST) {
            val pending = pendingUnlock
            pendingUnlock = null
            pending?.success(if (resultCode == RESULT_OK) null else "userCanceled")
            return
        }
        @Suppress("DEPRECATION")
        super.onActivityResult(requestCode, resultCode, data)
    }

    /// Copies a secret (backup text, recovery code) flagged as sensitive, so Android
    /// doesn't show it in the copy preview or keyboard suggestions (Android 13+ honour
    /// the flag; older versions read the same extra name).
    private fun copySensitive(text: String) {
        val clip = ClipData.newPlainText("", text)
        clip.description.extras = PersistableBundle().apply {
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
                putBoolean(ClipDescription.EXTRA_IS_SENSITIVE, true)
            } else {
                putBoolean("android.content.extra.IS_SENSITIVE", true)
            }
        }
        getSystemService(ClipboardManager::class.java)?.setPrimaryClip(clip)
    }

    /// Clears the clipboard if it still holds `text`. In the background Android 10+
    /// won't let the app read it; then it is cleared anyway, as it most likely still
    /// holds the secret.
    private fun clearClipboardIfUnchanged(text: String) {
        val clipboard = getSystemService(ClipboardManager::class.java) ?: return
        val current = clipboard.primaryClip?.takeIf { it.itemCount > 0 }?.getItemAt(0)?.text
        if (current != null && current.toString() != text) return
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.P) {
            clipboard.clearPrimaryClip()
        } else {
            clipboard.setPrimaryClip(ClipData.newPlainText("", ""))
        }
    }

    override fun configureFlutterEngine(flutterEngine: FlutterEngine) {
        super.configureFlutterEngine(flutterEngine)
        loadPaymentSounds()
        // The unlock prompt for approving, sending and the app lock.
        MethodChannel(flutterEngine.dartExecutor.binaryMessenger, "xyz.zafe/device_auth")
            .setMethodCallHandler { call, result ->
                when (call.method) {
                    "hasScreenLock" -> result.success(
                        getSystemService(KeyguardManager::class.java)?.isDeviceSecure == true,
                    )
                    "authenticate" -> runOnUiThread {
                        unlock(call.argument<String>("reason") ?: "", result)
                    }
                    else -> result.notImplemented()
                }
            }
        // Secrets copied by the app: flagged sensitive, cleared again after a while.
        MethodChannel(flutterEngine.dartExecutor.binaryMessenger, "xyz.zafe/clipboard")
            .setMethodCallHandler { call, result ->
                val text = call.argument<String>("text") ?: ""
                when (call.method) {
                    "copySensitive" -> runOnUiThread {
                        copySensitive(text)
                        result.success(null)
                    }
                    "clearIfUnchanged" -> runOnUiThread {
                        clearClipboardIfUnchanged(text)
                        result.success(null)
                    }
                    else -> result.notImplemented()
                }
            }
        // Payment moments (approve, ready, sent, received, failed): sound + haptics.
        MethodChannel(flutterEngine.dartExecutor.binaryMessenger, "xyz.zafe/payment_feedback")
            .setMethodCallHandler { call, result ->
                when (call.method) {
                    "play" -> {
                        val moment = call.argument<String>("moment") ?: ""
                        val sound = call.argument<Boolean>("sound") ?: true
                        runOnUiThread { result.success(playPayment(moment, sound)) }
                    }
                    else -> result.notImplemented()
                }
            }
        // Screens showing secrets (invites, backups) block screenshots, screen recording
        // and the recent-apps thumbnail while they are open.
        MethodChannel(flutterEngine.dartExecutor.binaryMessenger, "xyz.zafe/secure_screen")
            .setMethodCallHandler { call, result ->
                when (call.method) {
                    "setSecure" -> {
                        val secure = call.arguments as? Boolean ?: false
                        runOnUiThread {
                            secureRequested = secure
                            if (secure) {
                                window.addFlags(WindowManager.LayoutParams.FLAG_SECURE)
                            } else {
                                window.clearFlags(WindowManager.LayoutParams.FLAG_SECURE)
                            }
                        }
                        result.success(null)
                    }
                    else -> result.notImplemented()
                }
            }
        // The app's light/dark setting. The system splash is drawn before Flutter starts,
        // so it follows the OS theme unless the app's own night mode is set (Android 12+;
        // the OS persists it across launches). Older versions keep the OS theme.
        MethodChannel(flutterEngine.dartExecutor.binaryMessenger, "xyz.zafe/window_appearance")
            .setMethodCallHandler { call, result ->
                when (call.method) {
                    "setBrightness" -> {
                        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
                            val mode = when (call.argument<String>("brightness")) {
                                "dark" -> UiModeManager.MODE_NIGHT_YES
                                "light" -> UiModeManager.MODE_NIGHT_NO
                                else -> UiModeManager.MODE_NIGHT_AUTO
                            }
                            getSystemService(UiModeManager::class.java)
                                ?.setApplicationNightMode(mode)
                        }
                        result.success(null)
                    }
                    else -> result.notImplemented()
                }
            }
        // Opens the system's security settings (to set a screen lock); falls back to the
        // main Settings screen where a vendor doesn't have that page.
        MethodChannel(flutterEngine.dartExecutor.binaryMessenger, "xyz.zafe/system_settings")
            .setMethodCallHandler { call, result ->
                when (call.method) {
                    "openSecurity" -> {
                        val opened = listOf(
                            Settings.ACTION_SECURITY_SETTINGS,
                            Settings.ACTION_SETTINGS,
                        ).any { action ->
                            try {
                                startActivity(Intent(action))
                                true
                            } catch (e: ActivityNotFoundException) {
                                false
                            }
                        }
                        result.success(opened)
                    }
                    else -> result.notImplemented()
                }
            }
    }

    private companion object {
        const val UNLOCK_REQUEST = 0x2afe
    }
}
