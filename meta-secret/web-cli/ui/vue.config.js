module.exports = {
    configureWebpack: {
        experiments: {
            asyncWebAssembly: true,
            importAsync: true
        }
    }
}