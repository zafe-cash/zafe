package xyz.zafe.zafe

import android.app.UiModeManager
import android.content.ActivityNotFoundException
import android.content.Intent
import android.media.AudioAttributes
import android.media.AudioManager
import android.media.SoundPool
import android.os.Build
import android.os.Bundle
import android.provider.Settings
import android.view.HapticFeedbackConstants
import android.view.WindowManager
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

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
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

    override fun configureFlutterEngine(flutterEngine: FlutterEngine) {
        super.configureFlutterEngine(flutterEngine)
        loadPaymentSounds()
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
}
