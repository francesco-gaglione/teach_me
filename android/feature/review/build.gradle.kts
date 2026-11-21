plugins {
    id("teachme.android.feature")
}

android {
    namespace = "com.teachme.feature.review"
}

dependencies {
    implementation(project(":sdui"))
}
