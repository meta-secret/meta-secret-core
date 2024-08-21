#### Remote Split

One possible way to split passwords is by doing it remotely (vault-server will split your password)

Split:
 - deviceA generate a `Data Encryption Key` DEK
 - distributes the DEK across other devices (deviceB, deviceC), by using:
   -  `distribute()` method on server, by encrypting the DEK with others devices RSA keys and sending them to vault-server
   -  providing a QR of the DEK to scan by the other devices

 - when the DEK is distributed across devices:
   - deviceA uses AES to encrypt a password with the DEK
   - deviceA sends encrypted password to vault-server
   - vault server splits encrypted password
   - vault server distributes shares across devices:
     - encrypt each share of encrypted password with device's RSA public key accordingly
     - send each encrypted share of the password to devices accordingly  

Recover:
 - deviceA asking deviceB to provide a second share of the password
 - having enough (two in our case) shares deviceA can recover encrypted password\
 - then just decrypt the password with DEK
