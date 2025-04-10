
<p align="center">
  <img alt="Meta Secret" src="https://github.com/meta-secret/meta-secret-core/blob/main/docs/img/meta-secret-logo.png" width="100" />
  
  <h4 align="center"> <a href="https://apps.apple.com/app/metasecret/id1644286751">Meta Secret Mobile Application</a></h4>
  
  <h5 align="center"> <a href="https://meta-secret.org">Web site</a> </h5>
  <h5 align="center"> <a href="https://meta-secret.github.io">Meta Secret Web App</a> </h5>
</p>

### MetaSecret Application 
Meta Secret is a decentralised password manager that uses advanced encryption and decentralised storage to securely store and manage user data.

Meta Secret does not rely on a master password to grant access to passwords.

Instead, it uses a combination of biometric authentication and secret sharing techniques to provide secure access to user's confidential information.

Meta Secret is designed to operate on your device(s), the data is encrypted to ensure your information remains private and protected, enabling users to access their passwords from any device without compromising security.

Meta Secret also features a decentralised and open-source infrastructure, providing increased security and privacy for users.

### Application Design

#### Application Structure
![meta secret app picture](docs/img/app/meta-secret-app.png)

#### Password Split
![password split picture](docs/img/app/secret-split.png)

#### Password Recovery
![password recovery picture](docs/img/app/secret-recovery.png)


<br>
<br>

#### Activity Diagram
```mermaid
graph TD
    User --> |split password| MSS{MetaSecret}
    MSS --> |split| Hash1
    MSS --> |split| Hash2
    MSS --> |split| Hash3
    
    User --> |recover password| MSR{MetaSecret}
    MSR --> |read| HH1[Hash1]
    MSR --> |read| HH2[Hash2]
    HH1 --> RecoverAlgo[Meta Secret: Recovery Algorithm]
    HH2 --> RecoverAlgo
    RecoverAlgo --> RP[Recovered Password]
```

#### Sequence Diagram
```mermaid
sequenceDiagram
    note over User: Split to 3 shares
    User->>+MetaSecret: Split password
    
    MetaSecret->>User: show qr1 (of hash1)
    MetaSecret->>User: show qr2 (of hash2)
    MetaSecret->>-User: show qr3 (of hash3)
    User ->> World: save qr codes in different places

    note over User: Recover from 2 shares
    User ->> World: get qr1
    User ->> World: get qr3

    User ->> MetaSecret: recover password
    User -->> MetaSecret: provide qr1
    User -->> MetaSecret: provide qr3
    MetaSecret ->> MetaSecret: recover password
    MetaSecret ->> User: show password
```

## Web Application
Meta Secret Web Cli is available on https://meta-secret.github.io

## Command Line App

#### Split secrets:
You can split and restore your secrets by using meta-secret cli app in docker.
<br>
Imagine that we want to split `top$ecret`, then the command will be: 

```bash
$ mkdir secrets
$ docker run -ti --rm -v "$(pwd)/secrets:/app/secrets" ghcr.io/meta-secret/cli:latest split --secret top$ecret 
```

It will generate json/qr(jpg) files (shares of your secert) in the `secrets` directory.

#### Restore secrets:
When it comes to restore the secret, put json or qr files (shares of your secret) into the `secrets` directory.
Then run in case of qr (if you want restore from json, just pass --from json ):

```bash
$ docker run -ti --rm -v "$(pwd)/secrets:/app/secrets" ghcr.io/meta-secret/cli:latest restore --from qr 
```

## Advice for VPS-users
If you don't want to use FileZilla to download QR-codes to see on your computer, you can see them in terminal.

#### Installation
```bash
$ brew install qrencode (MacOS)
$ apt-get install qrencode (Debian/Ubuntu)
$ dnf install qrencode (CentOS/Rocky/Alma)
```

#### Showing QR codes in terminal
```bash
$ qrencode -t ansiutf8 < meta-secret-1.json
```

Congrats! Save these codes in secure place!

Below is optional
If you would like to extract data from QR's
  * Just take a phone to scan QR
  * or screenshot the terminal and upload it on this website: [webqr.com](https://webqr.com)

<br>

## Infrastructure Build

### Using Earthly for builds

The project uses Earthly for its build system. When building components that require API keys (like the AI-assisted development tools), you can provide them in several ways:

1. **Using a .env file** (simplest approach):
   ```bash
   # Copy the example file
   cp infra/.env.example infra/.env
   
   # Edit the .env file with your API key
   # Then run the build
   earthly +build-taskomatic-ai
   ```

2. **Using command line arguments**:
   ```bash
   earthly +build-taskomatic-ai --ANTHROPIC_API_KEY="your_api_key_here"
   ```

3. **Using environment variables**:
   ```bash
   export ANTHROPIC_API_KEY="your_api_key_here"
   earthly +build-taskomatic-ai
   ```

Earthly will automatically pick up the API key from any of these sources. Command line arguments take precedence over environment variables and .env file values.

**Note**: Never commit API keys or credentials to version control. The .env file is included in .gitignore.

<br>

# Taskomatic AI Docker Image

This Docker image contains the [aider](https://github.com/paul-gauthier/aider) AI coding assistant configured to use Claude 3.5 Haiku.

## Requirements

- Docker installed
- An Anthropic API key

## Usage

Run the container with your Anthropic API key as an environment variable:

```bash
docker run -it --rm \
  -e ANTHROPIC_API_KEY=your-api-key-here \
  -v $(pwd):/workspace \
  localhost/taskomatic-ai:latest
```

### Additional options

The container is pre-configured with Claude 3.5 Haiku, but you can pass additional arguments to aider:

```bash
# Run with a directory mounted and specific files to edit
docker run -it --rm \
  -e ANTHROPIC_API_KEY=your-api-key-here \
  -v $(pwd):/workspace \
  -w /workspace \
  localhost/taskomatic-ai:latest your_file.py another_file.js
```

## Building the image

To build the image yourself:

```bash
earthly +build-taskomatic-ai
```

## Issues and improvements

If you encounter any issues or have suggestions for improvements, please report them to the project maintainers.
