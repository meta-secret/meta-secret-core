schema Metadata:
    name: str
    labels: Labels

schema Labels:
    type: str
    location: str


projectName = "meta-secret-server"
volumeName = "${projectName}-vol"
appPath = "/app/meta-server"
