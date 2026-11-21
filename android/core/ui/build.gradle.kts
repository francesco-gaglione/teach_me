plugins {
    id("teachme.android.library")
    id("teachme.android.compose")
}

android {
    namespace = "com.teachme.core.ui"
}

dependencies {
    api(project(":core:designsystem"))
    api(project(":core:model"))
    implementation(libs.androidx.activity.compose)
}
