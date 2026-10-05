import java.util.Base64
import java.util.Properties

plugins {
    id("com.android.application")
    id("kotlin-android")
    // The Flutter Gradle Plugin must be applied after the Android and Kotlin Gradle plugins.
    id("dev.flutter.flutter-gradle-plugin")
}

// Release signing: android/key.properties (storeFile, storePassword, keyAlias,
// keyPassword; gitignored), written by the release workflow from repository secrets.
// Without it, release builds are signed with the debug key (local testing only: such an
// APK can't update one signed with the real key).
val keyProperties = Properties().apply {
    val file = rootProject.file("key.properties")
    if (file.exists()) file.inputStream().use { load(it) }
}

// Invite App Links: the host comes from `--dart-define=ZAFE_LINK_HOST=<host>` (Flutter
// passes dart-defines to Gradle base64-encoded, comma-separated). Without one the https
// intent filter points at a reserved `.invalid` host, so it can never match a real link.
val dartDefines: Map<String, String> =
    (project.findProperty("dart-defines") as String?)
        ?.split(",")
        ?.mapNotNull { encoded ->
            runCatching { String(Base64.getDecoder().decode(encoded)) }.getOrNull()
        }
        ?.mapNotNull { define ->
            define.split("=", limit = 2).takeIf { it.size == 2 }?.let { it[0] to it[1] }
        }
        ?.toMap()
        ?: emptyMap()
val zafeLinkHost: String = dartDefines["ZAFE_LINK_HOST"].orEmpty().lowercase().also { host ->
    if (host.isNotEmpty() && !Regex("^[a-z0-9]([a-z0-9-]*[a-z0-9])?(\\.[a-z0-9]([a-z0-9-]*[a-z0-9])?)+$").matches(host)) {
        throw GradleException("ZAFE_LINK_HOST must be a bare host name like zafe.example, got '$host'")
    }
}

// Testnet builds (`--dart-define=ZAFE_NETWORK=test`) are a separate app, "Zafe Testnet"
// (`xyz.zafe.zafe.testnet`), so it installs next to the mainnet app and can never update
// it or share its data. Mainnet and regtest (development) builds are `xyz.zafe.zafe`.
val zafeTestnet: Boolean = dartDefines["ZAFE_NETWORK"] in setOf("test", "testnet")

android {
    namespace = "xyz.zafe.zafe"
    compileSdk = flutter.compileSdkVersion
    ndkVersion = flutter.ndkVersion

    compileOptions {
        // flutter_local_notifications needs java.time on older Android.
        isCoreLibraryDesugaringEnabled = true
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    kotlinOptions {
        jvmTarget = JavaVersion.VERSION_17.toString()
    }

    defaultConfig {
        applicationId = if (zafeTestnet) "xyz.zafe.zafe.testnet" else "xyz.zafe.zafe"
        manifestPlaceholders["zafeAppLabel"] = if (zafeTestnet) "Zafe Testnet" else "Zafe"
        minSdk = flutter.minSdkVersion
        targetSdk = flutter.targetSdkVersion
        versionCode = flutter.versionCode
        versionName = flutter.versionName
        manifestPlaceholders["zafeLinkHost"] = zafeLinkHost.ifEmpty { "links.zafe.invalid" }
    }

    signingConfigs {
        if (keyProperties.getProperty("storeFile") != null) {
            create("release") {
                storeFile = rootProject.file(keyProperties.getProperty("storeFile"))
                storePassword = keyProperties.getProperty("storePassword")
                keyAlias = keyProperties.getProperty("keyAlias")
                keyPassword = keyProperties.getProperty("keyPassword")
            }
        }
    }

    buildTypes {
        release {
            signingConfig = signingConfigs.findByName("release")
                ?: signingConfigs.getByName("debug")
        }
    }
}

flutter {
    source = "../.."
}

dependencies {
    coreLibraryDesugaring("com.android.tools:desugar_jdk_libs:2.1.5")
    // AppCompat window themes (res/values*/styles.xml): local_auth's biometric prompt
    // crashes on Android 8 and below under a platform theme.
    implementation("androidx.appcompat:appcompat:1.7.0")
}

// Push notifications (FCM) are enabled by dropping the Firebase project's
// google-services.json into android/app/. Without it the app still builds and relies on
// periodic background checks (docs/tracker.md, "Push").
if (file("google-services.json").exists()) {
    apply(plugin = "com.google.gms.google-services")
}
