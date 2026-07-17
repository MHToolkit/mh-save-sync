import org.jetbrains.kotlin.gradle.dsl.JvmTarget

plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
    id("org.jetbrains.kotlin.plugin.compose")
}

android {
    namespace = "org.mhtoolkit.savesync"
    compileSdk = 36

    defaultConfig {
        applicationId = "org.mhtoolkit.savesync"
        minSdk = 29
        targetSdk = 36
        versionCode = 2
        versionName = "0.1.0-alpha.1"
    }

    buildFeatures {
        compose = true
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
}
