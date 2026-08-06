import org.jetbrains.kotlin.gradle.dsl.JvmTarget

plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
    id("org.jetbrains.kotlin.plugin.compose")
}

val releaseSigningVariables = mapOf(
    "keystore" to "MH_SAVE_SYNC_ANDROID_KEYSTORE",
    "storePassword" to "MH_SAVE_SYNC_ANDROID_STORE_PASSWORD",
    "keyAlias" to "MH_SAVE_SYNC_ANDROID_KEY_ALIAS",
    "keyPassword" to "MH_SAVE_SYNC_ANDROID_KEY_PASSWORD",
)
val releaseSigningValues = releaseSigningVariables.mapValues { (_, environmentName) ->
    providers.environmentVariable(environmentName).orNull
}
val releaseSigningConfigured = releaseSigningValues.values.all { !it.isNullOrBlank() }
val releaseSigningPartiallyConfigured =
    releaseSigningValues.values.any { !it.isNullOrBlank() } && !releaseSigningConfigured
val androidVersionCode = providers.environmentVariable("MH_SAVE_SYNC_ANDROID_VERSION_CODE")
    .map { value ->
        value.toIntOrNull()?.takeIf { it > 0 }
            ?: throw GradleException("MH_SAVE_SYNC_ANDROID_VERSION_CODE must be a positive integer")
    }
    .getOrElse(5)
val androidVersionName = providers.environmentVariable("MH_SAVE_SYNC_ANDROID_VERSION_NAME")
    .map { value ->
        value.takeIf { it.isNotBlank() }
            ?: throw GradleException("MH_SAVE_SYNC_ANDROID_VERSION_NAME must not be blank")
    }
    .getOrElse("0.1.0-alpha.4")

if (releaseSigningPartiallyConfigured) {
    throw GradleException(
        "Android release signing is only partially configured; set all MH_SAVE_SYNC_ANDROID_* variables",
    )
}
gradle.taskGraph.whenReady {
    val releaseTaskScheduled = allTasks.any { task ->
        task.project == project && task.name.contains("release", ignoreCase = true)
    }
    if (releaseTaskScheduled && !releaseSigningConfigured) {
        throw GradleException(
            "Android release signing is not configured; use scripts/android-package-release.sh",
        )
    }
}

android {
    namespace = "org.mhtoolkit.savesync"
    compileSdk = 36

    defaultConfig {
        applicationId = "org.mhtoolkit.savesync"
        minSdk = 29
        targetSdk = 36
        versionCode = androidVersionCode
        versionName = androidVersionName
    }

    buildFeatures {
        compose = true
    }

    testOptions {
        unitTests.isIncludeAndroidResources = true
    }

    signingConfigs {
        create("release") {
            if (releaseSigningConfigured) {
                storeFile = file(requireNotNull(releaseSigningValues["keystore"]))
                storePassword = requireNotNull(releaseSigningValues["storePassword"])
                keyAlias = requireNotNull(releaseSigningValues["keyAlias"])
                keyPassword = requireNotNull(releaseSigningValues["keyPassword"])
            }
        }
    }

    buildTypes {
        getByName("release") {
            if (releaseSigningConfigured) {
                signingConfig = signingConfigs.getByName("release")
            }
        }
    }

    sourceSets["main"].jniLibs.srcDir(layout.buildDirectory.dir("generated/jniLibs"))

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

}

val buildRustAndroid by tasks.registering(Exec::class) {
    val repoRoot = rootProject.projectDir.parentFile.parentFile
    val output = layout.buildDirectory.dir("generated/jniLibs")
    val sdkRoot = System.getenv("ANDROID_SDK_ROOT")
        ?: System.getenv("ANDROID_HOME")
        ?: File(System.getProperty("user.home"), "Library/Android/sdk").absolutePath
    val ndkRoot = System.getenv("ANDROID_NDK_HOME")
        ?: File(sdkRoot, "ndk/28.2.13676358").absolutePath
    inputs.files(fileTree(repoRoot.resolve("crates")) { include("**/*.rs", "**/Cargo.toml") })
    outputs.dir(output)
    workingDir(repoRoot)
    environment("ANDROID_NDK_HOME", ndkRoot)
    commandLine(
        "cargo", "ndk", "-t", "arm64-v8a", "-o", output.get().asFile.absolutePath,
        "build", "-p", "save-client", "--release",
    )
}

tasks.named("preBuild").configure { dependsOn(buildRustAndroid) }

kotlin {
    compilerOptions {
        jvmTarget.set(JvmTarget.JVM_17)
    }
}

dependencies {
    val composeBom = platform("androidx.compose:compose-bom:2026.06.00")
    implementation(composeBom)
    androidTestImplementation(composeBom)
    implementation("androidx.activity:activity-compose:1.13.0")
    implementation("androidx.compose.foundation:foundation")
    implementation("androidx.compose.material3:material3")
    implementation("androidx.compose.ui:ui")
    implementation("androidx.compose.ui:ui-tooling-preview")
    implementation("androidx.core:core-ktx:1.17.0")
    implementation("androidx.lifecycle:lifecycle-runtime-ktx:2.10.0")
    implementation("androidx.work:work-runtime-ktx:2.11.0")
    debugImplementation("androidx.compose.ui:ui-tooling")
    testImplementation("junit:junit:4.13.2")
    testImplementation("org.json:json:20250517")
    testImplementation("androidx.test:core:1.7.0")
    testImplementation("androidx.work:work-testing:2.11.0")
    testImplementation("org.robolectric:robolectric:4.16.1")
}
