package org.mhtoolkit.savesync

import android.content.Context
import androidx.test.core.app.ApplicationProvider
import androidx.work.Configuration
import androidx.work.CoroutineWorker
import androidx.work.ExistingPeriodicWorkPolicy
import androidx.work.ExistingWorkPolicy
import androidx.work.OneTimeWorkRequestBuilder
import androidx.work.PeriodicWorkRequestBuilder
import androidx.work.WorkManager
import androidx.work.WorkerParameters
import androidx.work.impl.utils.SynchronousExecutor
import androidx.work.testing.WorkManagerTestInitHelper
import java.util.concurrent.TimeUnit
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

class MigrationFixtureWorker(context: Context, params: WorkerParameters) :
    CoroutineWorker(context, params) {
    override suspend fun doWork(): Result = Result.success()
}

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
class LegacyWorkMigrationTest {
    private lateinit var context: Context
    private lateinit var workManager: WorkManager

    @Before
    fun setUp() {
        context = ApplicationProvider.getApplicationContext()
        context.getSharedPreferences(SyncScheduler.PREFERENCES, Context.MODE_PRIVATE)
            .edit().clear().commit()
        WorkManagerTestInitHelper.initializeTestWorkManager(
            context,
            Configuration.Builder().setExecutor(SynchronousExecutor()).build(),
        )
        workManager = WorkManager.getInstance(context)
    }

    @Test
    fun `upgrade cancels both persisted legacy unique work rows once`() {
        workManager.enqueueUniquePeriodicWork(
            SyncScheduler.LEGACY_PERIODIC_NAME,
            ExistingPeriodicWorkPolicy.KEEP,
            PeriodicWorkRequestBuilder<MigrationFixtureWorker>(15, TimeUnit.MINUTES).build(),
        ).result.get()
        workManager.enqueueUniqueWork(
            SyncScheduler.LEGACY_IMMEDIATE_NAME,
            ExistingWorkPolicy.KEEP,
            OneTimeWorkRequestBuilder<MigrationFixtureWorker>().build(),
        ).result.get()

        SyncScheduler.migrateLegacyWorkManager(context)

        val periodic = workManager.getWorkInfosForUniqueWork(
            SyncScheduler.LEGACY_PERIODIC_NAME,
        ).get()
        val immediate = workManager.getWorkInfosForUniqueWork(
            SyncScheduler.LEGACY_IMMEDIATE_NAME,
        ).get()
        assertTrue(periodic.isNotEmpty() && periodic.all { it.state.isFinished })
        assertTrue(immediate.isNotEmpty() && immediate.all { it.state.isFinished })
        assertEquals(
            1,
            context.getSharedPreferences(SyncScheduler.PREFERENCES, Context.MODE_PRIVATE)
                .getInt(SyncScheduler.LEGACY_WORK_MIGRATION_VERSION, 0),
        )

        // A simulated second app start is a no-op and preserves the completed migration marker.
        SyncScheduler.migrateLegacyWorkManager(context)
        assertEquals(1, periodic.size)
        assertEquals(1, immediate.size)
    }
}
