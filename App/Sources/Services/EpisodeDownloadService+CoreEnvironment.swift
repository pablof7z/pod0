extension EpisodeDownloadService {
    func publishCoreDownloadEnvironment() {
        appStore?.sharedLibrary?.observeDownloadEnvironment(
            network: networkStatus,
            availableCapacityBytes: availableStorageCapacity
        )
    }
}
