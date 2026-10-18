import java.util.Properties

plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
    id("rust")
}

val tauriProperties = Properties().apply {
    val propFile = file("tauri.properties")
    if (propFile.exists()) {
        propFile.inputStream().use { load(it) }
    }
}

android {
    compileSdk = 36
    namespace = "org.zerodenet.znetsink"
    defaultConfig {
        manifestPlaceholders["usesCleartextTraffic"] = "false"
        applicationId = "org.zerodenet.znetsink"
        minSdk = 24
        targetSdk = 36
        versionCode = tauriProperties.getProperty("tauri.android.versionCode", "1").toInt()
        versionName = tauriProperties.getProperty("tauri.android.versionName", "1.0")
    }
    // The build helper supplies a fresh private directory. Pin the SDK Debug
    // signer to that directory instead of falling back to the runner's HOME.
    // Ordinary manual Gradle builds without this variable keep the SDK default.
    providers.environmentVariable("ANDROID_USER_HOME").orNull
        ?.takeIf { it.isNotBlank() }
        ?.let { androidUserHome ->
            signingConfigs.getByName("debug").storeFile =
                file(androidUserHome).resolve("debug.keystore")
        }
    buildTypes {
        getByName("debug") {
            // Development signing is isolated from the unsigned Release variant.
            // CI uses a fresh Android user directory and never retains this key.
            signingConfig = signingConfigs.getByName("debug")
            applicationIdSuffix = ".debug"
            versionNameSuffix = "-debug"
            manifestPlaceholders["usesCleartextTraffic"] = "false"
            isDebuggable = true
            isJniDebuggable = true
            isMinifyEnabled = false
            packaging {
                jniLibs.keepDebugSymbols.add("*/arm64-v8a/*.so")
                jniLibs.keepDebugSymbols.add("*/armeabi-v7a/*.so")
                jniLibs.keepDebugSymbols.add("*/x86/*.so")
                jniLibs.keepDebugSymbols.add("*/x86_64/*.so")
            }
        }
        getByName("release") {
            // Reviewable unsigned host artifact; signing is a separate, unauthorized step.
            signingConfig = null
            isMinifyEnabled = true
            proguardFiles(
                *fileTree(".") { include("**/*.pro") }
                    .plus(getDefaultProguardFile("proguard-android-optimize.txt"))
                    .toList().toTypedArray()
            )
        }
    }
    kotlinOptions {
        jvmTarget = "1.8"
    }
    buildFeatures {
        buildConfig = true
    }
}

rust {
    rootDirRel = "../../../"
}

dependencies {
    implementation("androidx.webkit:webkit:1.14.0")
    implementation("androidx.appcompat:appcompat:1.7.1")
    implementation("androidx.activity:activity-ktx:1.10.1")
    implementation("com.google.android.material:material:1.12.0")
    implementation("androidx.lifecycle:lifecycle-process:2.10.0")
    testImplementation("junit:junit:4.13.2")
    androidTestImplementation("androidx.test.ext:junit:1.1.4")
    androidTestImplementation("androidx.test.espresso:espresso-core:3.5.0")
}

apply(from = "tauri.build.gradle.kts")
