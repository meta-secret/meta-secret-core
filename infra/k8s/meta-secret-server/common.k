schema Metadata:
    name: str
    labels: Labels

schema Labels:
    type: str
    location: str


projectName = "meta-secret-server"
volumeName = "${projectName}-vol"
appPath = "/meta-secret/app/meta-server"
