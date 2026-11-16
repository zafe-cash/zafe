package xyz.zafe.zafe

import android.app.KeyguardManager
import android.app.UiModeManager
import android.content.ActivityNotFoundException
import android.content.ClipData
import android.content.ClipDescription
import android.content.ClipboardManager
import android.content.Intent
import android.graphics.Color
import android.media.AudioAttributes
import android.media.AudioManager
import android.media.SoundPool
import android.os.Build
import android.os.Bundle
import android.os.PersistableBundle
import android.provider.Settings
import android.view.HapticFeedbackConstants
import android.view.WindowManager
import androidx.activity.SystemBarStyle
import androidx.activity.enableEdgeToEdge
import androidx.activity.result.contract.ActivityResultContracts
import androidx.biometric.BiometricManager.Authenticators
import androidx.biometric.BiometricPrompt
import androidx.core.content.ContextCompat
import io.flutter.embedding.android.FlutterFragmentActivity
import io.flutter.embedding.engine.FlutterEngine
import io.flutter.plugin.common.MethodChannel

class MainActivity : FlutterFragmentActivity() {
    private var secureRequested = false

    private var soundPool: SoundPool? = null
    private val soundIds = mutableMapOf<String, Int>()
    private val loadedSounds = mutableSetOf<Int>()

    private var pendingUnlock: MethodChannel.Result? = null
    private val deviceCredentialUnlock =
        registerForActivityResult(ActivityResultContracts.StartActivityForResult()) { outcome ->
            val pending = pendingUnlock
            pendingUnlock = null
            pending?.success(if (outcome.resultCode == RESULT_OK) null else "userCanceled")
        }

    private var privacyChannel: MethodChannel? = null
    private var topResumed: Boolean? = null

    override fun onTopResumedActivityChanged(isTopResumedActivity: Boolean) {
        super.onTopResumedActivityChanged(isTopResumedActivity)
        topResumed = isTopResumedActivity
        privacyChannel?.invokeMethod("topResumed", isTopResumedActivity)
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        enableEdgeToEdge(
            statusBarStyle = SystemBarStyle.auto(Color.TRANSPARENT, Color.TRANSPARENT),
            navigationBarStyle = SystemBarStyle.auto(WINDOW_LIGHT, WINDOW_DARK),
        )
        super.onCreate(savedInstanceState)
        window.decorView.filterTouchesWhenObscured = true
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
            setRecentsScreenshotEnabled(false)
        }
    }

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
        val times = if (tap == HapticFeedbackConstants.REJECT) taps.take(1) else taps.toList()
        times.forEach { at -> view.postDelayed({ view.performHapticFeedback(tap) }, at) }
        return true
    }

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
            val intent = getSystemService(KeyguardManager::class.java)
                ?.createConfirmDeviceCredentialIntent("Unlock Zafe", reason)
            if (intent == null) {
                result.success("noCredentialsSet")
                return
            }
            pendingUnlock?.success("authInProgress")
            pendingUnlock = result
            deviceCredentialUnlock.launch(intent)
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
        privacyChannel = MethodChannel(flutterEngine.dartExecutor.binaryMessenger, "xyz.zafe/privacy")
            .also { channel ->
                channel.setMethodCallHandler { call, result ->
                    when (call.method) {
                        "isTopResumed" -> result.success(topResumed)
                        else -> result.notImplemented()
                    }
                }
            }
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
        const val WINDOW_LIGHT = 0xFFF1F5F5.toInt()
        const val WINDOW_DARK = 0xFF080B0B.toInt()
    }
}
