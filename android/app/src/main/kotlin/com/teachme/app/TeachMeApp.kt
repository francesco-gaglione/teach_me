package com.teachme.app

import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalWindowInfo
import androidx.hilt.lifecycle.viewmodel.compose.hiltViewModel
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.compose.LifecycleEventEffect
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.navigation.NavController
import androidx.navigation.NavDestination.Companion.hasRoute
import androidx.navigation.NavHostController
import androidx.navigation.compose.NavHost
import androidx.navigation.compose.composable
import androidx.navigation.compose.rememberNavController
import androidx.navigation.toRoute
import com.teachme.core.designsystem.WindowWidthClass
import com.teachme.core.designsystem.components.TeachScreenFrame
import com.teachme.core.designsystem.components.TeachToastState
import com.teachme.core.designsystem.currentWindowWidthClass
import com.teachme.core.model.AuthState
import com.teachme.core.model.Flash
import com.teachme.core.model.FlashInbox
import com.teachme.core.model.SignedOutReason
import com.teachme.core.model.TopicTabNames
import com.teachme.core.ui.LocalLlmKeyStatus
import com.teachme.core.ui.LocalOpenLlmKeySettings
import com.teachme.feature.folder.FolderPane
import com.teachme.feature.folder.TopicAskArgs
import com.teachme.feature.folder.TopicPane
import com.teachme.feature.home.HomePane
import com.teachme.feature.home.HomeViewModel
import com.teachme.feature.settings.SettingsArgs
import com.teachme.feature.lesson.LessonRoute as LessonScreenRoute
import com.teachme.feature.login.LoginRoute as LoginScreenRoute
import com.teachme.feature.review.ReviewRoute as ReviewScreenRoute
import com.teachme.feature.settings.ChangePasswordRoute as ChangePasswordScreenRoute
import com.teachme.feature.settings.LicensesRoute as LicensesScreenRoute
import com.teachme.feature.settings.MissingComponentsRoute as MissingComponentsScreenRoute
import com.teachme.feature.settings.SettingsRoute as SettingsScreenRoute
import com.teachme.feature.settings.SuspendedRoute as SuspendedScreenRoute

/** Below this usable width a large font scale makes two panes too tight (home.md, Expanded). */
private const val CRAMPED_WIDTH_DP = 1000
private const val CRAMPED_FONT_SCALE = 1.5f

/** List-detail on expanded windows only; large fonts on not-so-wide windows fall back to stacked. */
@Composable
private fun useListDetail(): Boolean {
    if (currentWindowWidthClass() != WindowWidthClass.Expanded) return false
    val density = LocalDensity.current
    val widthDp =
        with(density) {
            LocalWindowInfo.current.containerSize.width
                .toDp()
                .value
        }
    return !(widthDp < CRAMPED_WIDTH_DP && density.fontScale > CRAMPED_FONT_SCALE)
}

@Composable
fun TeachMeApp(
    inbox: FlashInbox,
    openReview: Boolean = false,
    onOpenReviewHandled: () -> Unit = {},
    mainViewModel: MainViewModel = hiltViewModel(),
) {
    val authState by mainViewModel.authState.collectAsStateWithLifecycle()
    LifecycleEventEffect(Lifecycle.Event.ON_START) { mainViewModel.onForeground() }
    // Wait for the stored session so the first frame is already the right destination.
    if (authState is AuthState.Loading) return
    val navController = rememberNavController()
    // The login is the first destination without a session; otherwise the stack the session ended over stays below it.
    val startDestination = remember { if (authState is AuthState.SignedOut) LoginRoute else HomeRoute }
    SessionNavigation(navController, authState)
    OpenReviewRequest(navController, authState, openReview, onOpenReviewHandled)

    val keyStatus by mainViewModel.llmKeyStatus.collectAsStateWithLifecycle()
    CompositionLocalProvider(
        LocalLlmKeyStatus provides keyStatus,
        LocalOpenLlmKeySettings provides { variant ->
            navController.navigate(SettingsRoute(SettingsArgs.SECTION_LLM_KEY, variant.name.lowercase())) {
                launchSingleTop = true
            }
        },
    ) {
        AppNavHost(navController, startDestination, inbox)
    }
}

/**
 * The tap on the review notification (ADR 0012 point 5): opens the existing review route over the current stack. Without
 * a session it waits: the login comes first and the request is served once the user is signed in. Declared after
 * [SessionNavigation] so that, on a sign-in, Home is in place before the review opens over it.
 */
@Composable
private fun OpenReviewRequest(
    navController: NavHostController,
    authState: AuthState,
    requested: Boolean,
    onHandled: () -> Unit,
) {
    LaunchedEffect(requested, authState is AuthState.SignedIn) {
        if (requested && authState is AuthState.SignedIn) {
            navController.navigate(ReviewRoute()) { launchSingleTop = true }
            onHandled()
        }
    }
}

/**
 * Reacts to the session (login.md, global rule for `401`): when it ends the login opens over the stack; after a sign-in
 * with the same user whose session ended the stack comes back as it was, with another user (or after "Esci") it
 * restarts from Home.
 */
@Composable
private fun SessionNavigation(
    navController: NavHostController,
    authState: AuthState,
) {
    // Who owned the stack when the session ended by itself; saved so a rotation does not forget it.
    var returnToUser by rememberSaveable { mutableStateOf<String?>(null) }
    LaunchedEffect(authState) {
        val onLogin = navController.currentDestination?.hasRoute<LoginRoute>() == true
        when (authState) {
            is AuthState.SignedOut -> {
                returnToUser = if (authState.reason == SignedOutReason.Expired) authState.user?.id else null
                if (!onLogin) {
                    navController.navigate(LoginRoute) {
                        launchSingleTop = true
                        // "Esci" leaves nothing behind it; an expired session keeps the stack for the same user.
                        if (authState.reason ==
                            SignedOutReason.LoggedOut
                        ) {
                            popUpTo(navController.graph.id) { inclusive = true }
                        }
                    }
                }
            }
            is AuthState.SignedIn ->
                if (onLogin) {
                    if (returnToUser != null &&
                        returnToUser == authState.session.user.id &&
                        navController.previousBackStackEntry != null
                    ) {
                        navController.popBackStack()
                    } else {
                        navController.navigate(HomeRoute) { popUpTo(navController.graph.id) { inclusive = true } }
                    }
                    returnToUser = null
                }
            AuthState.Loading -> Unit
        }
    }
}

@Composable
private fun AppNavHost(
    navController: NavHostController,
    startDestination: Any,
    inbox: FlashInbox,
) {
    NavHost(
        navController = navController,
        startDestination = startDestination,
    ) {
        composable<LoginRoute> { LoginScreenRoute(inbox) }
        composable<HomeRoute> { entry ->
            val listDetail = useListDetail()
            val homeViewModel: HomeViewModel = hiltViewModel(entry)
            val state by homeViewModel.state.collectAsStateWithLifecycle()
            var wasListDetail by rememberSaveable { mutableStateOf(listDetail) }
            LaunchedEffect(listDetail) {
                // Expanded -> compact: show the folder the user was looking at, with Home underneath.
                val selected = state.folders?.firstOrNull { it.id == state.selectedFolderId }
                if (wasListDetail && !listDetail && state.selectionByUser && selected != null) {
                    navController.navigate(FolderRoute(selected.id, selected.name)) { launchSingleTop = true }
                }
                wasListDetail = listDetail
            }
            if (listDetail) {
                LibraryScreen(
                    homeViewModel = homeViewModel,
                    initialFolderId = null,
                    onOpenTopic = { navController.navigate(TopicRoute(it.id, it.title)) { launchSingleTop = true } },
                    onOpenSettings = { navController.navigate(SettingsRoute()) { launchSingleTop = true } },
                    onOpenReview = { id, title ->
                        navController.navigate(ReviewRoute(id, title)) {
                            launchSingleTop =
                                true
                        }
                    },
                    onOpenLesson = { navController.navigate(LessonRoute(it)) { launchSingleTop = true } },
                )
            } else {
                val toast = remember { TeachToastState() }
                TeachScreenFrame {
                    HomePane(
                        onOpenFolder = {
                            homeViewModel.select(it.id)
                            navController.navigate(FolderRoute(it.id, it.name)) { launchSingleTop = true }
                        },
                        onOpenSettings = { navController.navigate(SettingsRoute()) { launchSingleTop = true } },
                        onOpenReview = { id, title ->
                            navController.navigate(ReviewRoute(id, title)) {
                                launchSingleTop =
                                    true
                            }
                        },
                        onOpenLesson = { navController.navigate(LessonRoute(it)) { launchSingleTop = true } },
                        toast = toast,
                        viewModel = homeViewModel,
                    )
                }
            }
        }
        composable<FolderRoute> { entry ->
            val route = entry.toRoute<FolderRoute>()
            if (useListDetail()) {
                // Rotated to expanded while Folder was on top: the list-detail on Home already shows it.
                // Select it there and pop, so Back does not land on an identical second Library.
                val homeEntry = remember(entry) { navController.getBackStackEntry<HomeRoute>() }
                val homeViewModel: HomeViewModel = hiltViewModel(homeEntry)
                LaunchedEffect(route.id) {
                    homeViewModel.select(route.id)
                    navController.popBackStack()
                }
            } else {
                val toast = remember { TeachToastState() }
                TeachScreenFrame {
                    FolderPane(
                        folderId = route.id,
                        folderName = route.name,
                        onBack = { navController.popBackStack() },
                        onOpenTopic = {
                            navController.navigate(
                                TopicRoute(it.id, it.title),
                            ) { launchSingleTop = true }
                        },
                        onLeave = { flash -> navController.leaveWith(inbox, flash) },
                        onOpenSettings = { navController.navigate(SettingsRoute()) { launchSingleTop = true } },
                        toast = toast,
                    )
                }
            }
        }
        composable<TopicRoute> { entry ->
            val route = entry.toRoute<TopicRoute>()
            val toast = remember { TeachToastState() }
            TeachScreenFrame {
                TopicPane(
                    topicId = route.id,
                    title = route.title,
                    onBack = { navController.popBackStack() },
                    onLeave = { flash -> navController.leaveWith(inbox, flash) },
                    onOpenSettings = { navController.navigate(SettingsRoute()) { launchSingleTop = true } },
                    toast = toast,
                    onOpenLesson = { navController.navigate(LessonRoute(it)) { launchSingleTop = true } },
                    onOpenReview = { id, title ->
                        navController.navigate(ReviewRoute(id, title)) {
                            launchSingleTop =
                                true
                        }
                    },
                    initialTab = route.tab,
                    ask =
                        route.lessonId?.let {
                            TopicAskArgs(
                                route.prompt,
                                it,
                                route.lessonNumber,
                                route.lessonTitle.orEmpty(),
                                route.lessonMinutes,
                            )
                        },
                )
            }
        }
        composable<MissingComponentsRoute> {
            MissingComponentsScreenRoute(
                onBack = { navController.popBackStack() },
                onOpenSettings = { navController.popBackStack<SettingsRoute>(inclusive = false) },
            )
        }
        composable<SuspendedRoute> { SuspendedScreenRoute(onBack = { navController.popBackStack() }) }
        composable<LicensesRoute> { LicensesScreenRoute(onBack = { navController.popBackStack() }) }
        composable<ChangePasswordRoute> { ChangePasswordScreenRoute(onBack = { navController.popBackStack() }) }
        composable<SettingsRoute> {
            SettingsScreenRoute(
                inbox = inbox,
                onBack = { navController.popBackStack() },
                onOpenPassword = { navController.navigate(ChangePasswordRoute) { launchSingleTop = true } },
                onOpenSuspended = { navController.navigate(SuspendedRoute) { launchSingleTop = true } },
                onOpenMissingComponents = { navController.navigate(MissingComponentsRoute) { launchSingleTop = true } },
                onOpenLicenses = { navController.navigate(LicensesRoute) { launchSingleTop = true } },
                extraContent = {
                    DebugLessonsSection(onOpenLesson = {
                        navController.navigate(LessonRoute(it)) {
                            launchSingleTop =
                                true
                        }
                    })
                },
            )
        }
        composable<ReviewRoute> {
            ReviewScreenRoute(
                onClose = { navController.popBackStack() },
                onOpenSettings = { navController.navigate(SettingsRoute()) { launchSingleTop = true } },
                // The lesson opens over the review: Back returns to the same card, still on its back (review.md 4.6).
                onOpenLesson = { navController.navigate(LessonRoute(it)) { launchSingleTop = true } },
            )
        }
        composable<LessonRoute> {
            LessonScreenRoute(
                onClose = { navController.popBackStack() },
                // The lesson stays in the stack under the Topic screen: Back returns to it where it was (ask_agent.md).
                onAskAgent = { request ->
                    navController.navigate(
                        TopicRoute(
                            id = request.topicId,
                            title = "",
                            tab = TopicTabNames.CHAT,
                            prompt = request.prompt,
                            lessonId = request.lessonId,
                            lessonNumber = request.lessonNumber,
                            lessonTitle = request.lessonTitle,
                        ),
                    ) { launchSingleTop = true }
                },
                onOpenLesson = { navController.navigate(LessonRoute(it)) { launchSingleTop = true } },
            )
        }
    }
}

/** Leaves a message for the screen revealed by the pop, then pops. */
private fun NavController.leaveWith(
    inbox: FlashInbox,
    flash: Flash,
) {
    inbox.post(flash)
    popBackStack()
}
