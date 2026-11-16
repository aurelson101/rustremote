#!/usr/bin/env python3
"""Loopback TLS/CredSSP simulation, no Windows account, desktop, shell or RDP graphics.
Only --native on ephemeral GitHub Windows CI may temporarily trust this fixture's CA.
All credentials are generated in memory; only a private fake NT hash is written.
"""
import argparse
import datetime
import ipaddress
import json
import logging
import os
from pathlib import Path
import secrets
import socket
import ssl
import struct
import subprocess
import sys
import tempfile
import threading
import time

from cryptography import x509
from cryptography.hazmat.primitives import hashes, serialization
from cryptography.hazmat.primitives.asymmetric import rsa
from cryptography.x509.oid import NameOID, ExtendedKeyUsageOID
# CI-only dependency, pinned wheel; no change to the application's runtime or system Python.
_dependency_guard=None
if "--fetch-deps" in sys.argv:
    import hashlib, urllib.request, zipfile, io
    _dependency_guard=tempfile.TemporaryDirectory(prefix="RustRemote NLA dependency ")
    url="https://files.pythonhosted.org/packages/eb/4e/f6c1bb147890f246c3a24ce836be4f6cfc44fd7be08e0a229e3a1c08b560/pyspnego-0.12.4-py3-none-any.whl"
    with urllib.request.urlopen(url,timeout=30) as response: data=response.read(2*1024*1024+1)
    if hashlib.sha256(data).hexdigest()!="a29a34de1abe9e9b30d9aa1d363f8e49fc159eb7355b1f2ee86ccb3a2ce3bba9":raise RuntimeError("CredSSP wheel digest mismatch")
    with zipfile.ZipFile(io.BytesIO(data)) as archive:
        if len(archive.infolist())>256 or sum(item.file_size for item in archive.infolist())>8*1024*1024:raise RuntimeError("Dependency archive exceeds budget")
        for item in archive.infolist():
            path=Path(item.filename)
            if path.is_absolute() or ".." in path.parts or "\\" in item.filename:raise RuntimeError("Unsafe dependency member")
        archive.extractall(_dependency_guard.name)
    sys.path.insert(0,_dependency_guard.name)
import spnego
from spnego._negotiate import NegotiateProxy
from spnego._ntlm_raw.crypto import ntowfv1
from spnego.tls import CredSSPTLSContext, get_certificate_public_key

logging.getLogger("spnego").addHandler(logging.NullHandler())
logging.getLogger("spnego").propagate = False
os.umask(0o077)
USER, DOMAIN = "rr-lab", "RRTEST"


class LabNegotiate(NegotiateProxy):
    @classmethod
    def available_protocols(cls, options=None):
        # Never consult Kerberos or Windows SSPI account databases on this simulator.
        return ["ntlm", "negotiate"]


def negotiation(usage, password=None):
    return LabNegotiate(username=DOMAIN + "\\" + USER if password else None,
                        password=password, usage=usage, options=spnego.NegotiateOptions.use_ntlm)


def certificate(root):
    now = datetime.datetime.now(datetime.timezone.utc)
    key = rsa.generate_private_key(public_exponent=65537, key_size=2048)
    subject = x509.Name([x509.NameAttribute(NameOID.COMMON_NAME, "RustRemote loopback lab " + secrets.token_hex(8))])
    ca = (x509.CertificateBuilder().subject_name(subject).issuer_name(subject).public_key(key.public_key())
          .serial_number(x509.random_serial_number()).not_valid_before(now-datetime.timedelta(minutes=5))
          .not_valid_after(now+datetime.timedelta(hours=2)).add_extension(x509.BasicConstraints(ca=True,path_length=0),critical=True)
          .add_extension(x509.KeyUsage(True,False,False,False,False,True,True,False,False),critical=True)
          .sign(key,hashes.SHA256()))
    leaf_key = rsa.generate_private_key(public_exponent=65537,key_size=2048)
    leaf = (x509.CertificateBuilder().subject_name(x509.Name([x509.NameAttribute(NameOID.COMMON_NAME,"127.0.0.1")]))
            .issuer_name(subject).public_key(leaf_key.public_key()).serial_number(x509.random_serial_number())
            .not_valid_before(now-datetime.timedelta(minutes=5)).not_valid_after(now+datetime.timedelta(hours=1))
            .add_extension(x509.BasicConstraints(ca=False,path_length=None),critical=True)
            .add_extension(x509.ExtendedKeyUsage([ExtendedKeyUsageOID.SERVER_AUTH]),critical=False)
            .add_extension(x509.SubjectAlternativeName([x509.IPAddress(ipaddress.ip_address("127.0.0.1"))]),critical=False)
            .sign(key,hashes.SHA256()))
    (root/"ca.cer").write_bytes(ca.public_bytes(serialization.Encoding.DER))
    (root/"ca.pem").write_bytes(ca.public_bytes(serialization.Encoding.PEM))
    chain=root/"server.pem"
    chain.write_bytes(leaf.public_bytes(serialization.Encoding.PEM)+ca.public_bytes(serialization.Encoding.PEM)+
                      leaf_key.private_bytes(serialization.Encoding.PEM,serialization.PrivateFormat.PKCS8,serialization.NoEncryption()))
    context=ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
    context.minimum_version=ssl.TLSVersion.TLSv1_2
    context.maximum_version=ssl.TLSVersion.TLSv1_2
    context.set_ciphers("ECDHE-RSA-AES256-GCM-SHA384:ECDHE-RSA-AES128-GCM-SHA256")
    context.load_cert_chain(chain)
    return CredSSPTLSContext(context, get_certificate_public_key(leaf.public_bytes(serialization.Encoding.DER))), ca.fingerprint(hashes.SHA1()).hex().upper()


class ExistingLabCertificate(RuntimeError):
    pass


def powershell(script, root, thumb=""):
    path=Path(os.environ.get("RUSTREMOTE_LAB_PWSH",str(Path(os.environ.get("SystemRoot","C:/Windows"))/"System32/WindowsPowerShell/v1.0/powershell.exe")))
    if not path.is_absolute() or not path.is_file(): raise RuntimeError("Trusted PowerShell unavailable")
    result=subprocess.run([str(path),"-NoProfile","-NonInteractive","-Command",script],
                          env=dict(os.environ,RUSTREMOTE_NLA_ROOT=str(root),RUSTREMOTE_NLA_THUMB=thumb),
                          capture_output=True,text=True,timeout=30)
    if result.returncode==17: raise ExistingLabCertificate("Fixture certificate already existed")
    if result.returncode: raise RuntimeError("Private laboratory setup or certificate cleanup failed")


def secure_folder(root):
    powershell("""$ErrorActionPreference='Stop'
$sid=[Security.Principal.WindowsIdentity]::GetCurrent().User
$acl=[Security.AccessControl.DirectorySecurity]::new()
$acl.SetOwner($sid); $acl.SetAccessRuleProtection($true,$false)
foreach($who in @($sid,[Security.Principal.SecurityIdentifier]::new('S-1-5-18'))){
$acl.AddAccessRule([Security.AccessControl.FileSystemAccessRule]::new($who,'FullControl','ContainerInherit,ObjectInherit','None','Allow'))}
Set-Acl -LiteralPath $env:RUSTREMOTE_NLA_ROOT -AclObject $acl
""",root)


def trust(root, thumb, add):
    script = """$ErrorActionPreference='Stop'
$store=[Security.Cryptography.X509Certificates.X509Store]::new('Root','CurrentUser')
$store.Open([Security.Cryptography.X509Certificates.OpenFlags]::ReadWrite)
try {
$certificate=[Security.Cryptography.X509Certificates.X509Certificate2]::new([IO.File]::ReadAllBytes((Join-Path $env:RUSTREMOTE_NLA_ROOT 'ca.cer')))
if($certificate.Thumbprint -ne $env:RUSTREMOTE_NLA_THUMB){throw 'Fixture identity mismatch'}
$entries=$store.Certificates.Find([Security.Cryptography.X509Certificates.X509FindType]::FindByThumbprint,$env:RUSTREMOTE_NLA_THUMB,$false)
__PRECHECK__
__OPERATION__
$entries=$store.Certificates.Find([Security.Cryptography.X509Certificates.X509FindType]::FindByThumbprint,$env:RUSTREMOTE_NLA_THUMB,$false)
if($entries.Count -ne __COUNT__){throw 'Certificate store verification failed'}
} finally {$store.Close()}
"""
    script=script.replace("__PRECHECK__","if($entries.Count -ne 0){exit 17}" if add else "")
    script=script.replace("__OPERATION__","$store.Add($certificate)" if add else "foreach($entry in $entries){$store.Remove($entry)}")
    powershell(script.replace("__COUNT__","1" if add else "0"),root,thumb)


def receive(stream, length):
    result=bytearray()
    while len(result)<length:
        chunk=stream.recv(length-len(result))
        if not chunk: raise EOFError()
        result.extend(chunk)
    return bytes(result)


def tls_handshake(stream, context):
    # CredSSP's token API assumes complete TLS flights (e.g. WinRM HTTP framing).
    # Use its existing OpenSSL BIO for TCP handshake fragmentation, never a timing delay.
    engine=context.get_extra_info("ssl_object")
    budget=0
    for _ in range(64):
        complete=False
        try:engine.do_handshake();complete=True
        except ssl.SSLWantReadError:pass
        output=context._out_buff.read()
        if output:stream.sendall(output)
        if complete:return
        header=receive(stream,5);length=int.from_bytes(header[3:],"big")
        budget+=length+5
        if header[0] not in (20,21,22,23) or length>18432 or budget>256*1024:raise ValueError("TLS fixture budget")
        context._in_buff.write(header+receive(stream,length))
    raise ValueError("TLS fixture round budget")


class Peer:
    def __init__(self, context, root, password):
        self.listener=socket.socket();self.listener.bind(("127.0.0.1",0));self.listener.listen(1);self.listener.settimeout(25)
        self.port=self.listener.getsockname()[1];self.context=context;self.root=root;self.password=password
        self.state={"negotiation":False,"credssp_complete":False,"password_match":False,"account_match":False}
        self.worker=threading.Thread(target=self.serve);self.worker.start()

    def serve(self):
        try:
            with self.listener.accept()[0] as stream:
                stream.settimeout(15)
                header=receive(stream,4);length=int.from_bytes(header[2:],"big")
                if header[:2]!=b"\x03\x00" or not 11<=length<=4096: raise ValueError()
                request=header+receive(stream,length-4)
                offset=11
                if request[offset:].startswith(b"Cookie:"):
                    end=request.find(b"\r\n",offset,1024)
                    if end<0:raise ValueError()
                    offset=end+2
                negotiation_request=request[offset:offset+8]
                if request[5]!=0xe0 or len(negotiation_request)!=8 or negotiation_request[:1]!=b"\x01" or negotiation_request[2:4]!=b"\x08\0" or not int.from_bytes(negotiation_request[4:],"little") & 2:raise ValueError()
                stream.sendall(bytes.fromhex("030000130ed000000000000200080002000000"))
                self.state["negotiation"]=True
                context=spnego.server(protocol="credssp",credssp_negotiate_context=negotiation("accept"),
                                      credssp_tls_context=self.context,credssp_min_protocol=5)
                tls_handshake(stream,context)
                context.step()
                budget=0
                for _ in range(64):
                    header=receive(stream,5);length=int.from_bytes(header[3:],"big")
                    budget+=length+5
                    if header[0] not in (20,21,22,23) or length>18432 or budget>256*1024: raise ValueError()
                    token=context.step(header+receive(stream,length))
                    if token:stream.sendall(token)
                    if context.complete:
                        credential=context.get_extra_info("client_credential")
                        self.state.update(credssp_complete=True,
                            account_match=credential.domain_name.upper()==DOMAIN and credential.username==USER,
                            password_match=secrets.compare_digest(credential.password.encode(),self.password.encode()))
                        break
        except Exception as error:
            self.state["error_class"]=type(error).__name__
        finally:
            state=self.root/"rdp-nla-state.json"
            temporary=state.with_suffix(".tmp")
            temporary.write_text('{"done":true}',encoding="utf-8");temporary.replace(state)
            self.listener.close()

    def close(self):
        self.listener.close();self.worker.join(timeout=30)
        if self.worker.is_alive():raise RuntimeError("RDP fixture did not stop")


def verify_fixture(peer, root, password):
    context=ssl.SSLContext(ssl.PROTOCOL_TLS_CLIENT);context.check_hostname=False;context.verify_mode=ssl.CERT_REQUIRED
    context.minimum_version=ssl.TLSVersion.TLSv1_2;context.load_verify_locations(root/"ca.pem")
    client=spnego.client(DOMAIN+"\\"+USER,password,protocol="credssp",credssp_negotiate_context=negotiation("initiate",password),
                         credssp_tls_context=CredSSPTLSContext(context),credssp_min_protocol=5)
    with socket.create_connection(("127.0.0.1",peer.port),timeout=10) as stream:
        stream.settimeout(10)
        stream.sendall(bytes.fromhex("030000130ee000000000000100080003000000"));assert receive(stream,19)[-4:]==b"\x02\0\0\0"
        tls_handshake(stream,client)
        token=client.step()
        for _ in range(64):
            if token:stream.sendall(token)
            if client.complete:break
            header=receive(stream,5);token=client.step(header+receive(stream,int.from_bytes(header[3:],"big")))
        assert client.complete


def main():
    parser=argparse.ArgumentParser();parser.add_argument("--verify-fixture",action="store_true");parser.add_argument("--native",type=Path);parser.add_argument("--fetch-deps",action="store_true");args=parser.parse_args()
    if not args.verify_fixture and not (args.native and sys.platform=="win32" and os.environ.get("GITHUB_ACTIONS")=="true"):
        parser.error("Native certificate changes are restricted to an ephemeral GitHub Windows runner")
    trusted=False;peer=None
    with tempfile.TemporaryDirectory(prefix="RustRemote NLA ") as folder:
        root=Path(folder)
        if sys.platform=="win32":secure_folder(root)
        password=secrets.token_urlsafe(24)+"é"
        user_file=root/"ntlm-cache"
        user_file.write_text(f"{DOMAIN}\\{USER}:0:{'0'*32}:{ntowfv1(password).hex()}:[U]:LCT-0\n",encoding="utf-8")
        previous=os.environ.get("NTLM_USER_FILE");os.environ["NTLM_USER_FILE"]=str(user_file)
        try:
            context,thumb=certificate(root)
            if not args.verify_fixture:
                trusted=True
                try:trust(root,thumb,True)
                except ExistingLabCertificate:
                    trusted=False
                    raise
            peer=Peer(context,root,password)
            if args.verify_fixture:verify_fixture(peer,root,password)
            else:
                profile=dict(name="CredSSP loopback lab",host="127.0.0.1",port=peer.port,username=USER,favorite=False,
                    options=dict(protocol="Rdp",domain=DOMAIN,width=1280,height=720,fullscreen=False,clipboard=False,clipboard_files=False))
                result=subprocess.run([str(args.native.resolve(strict=True)),"--self-test-rdp-nla"],
                    input=json.dumps(dict(profile=profile,secret=password,state=str(root/"rdp-nla-state.json"))),
                    text=True,encoding="utf-8",capture_output=True,timeout=40)
                if result.returncode:raise RuntimeError("Native RDP lab client did not complete")
            peer.close()
            if not all(peer.state.get(key) for key in ("negotiation","credssp_complete","account_match","password_match")):
                raise RuntimeError("Native RDP profile credential was not authenticated")
            report=dict(result="PASS",target="127.0.0.1",real_windows_account=False,desktop_session=False,**peer.state)
        except Exception:
            if peer: print(json.dumps({"stage":"CredSSP", "events":peer.state}))
            raise
        finally:
            try:
                if peer and peer.worker.is_alive():peer.close()
            finally:
                try:
                    if trusted:trust(root,thumb,False)
                finally:
                    if previous is None:os.environ.pop("NTLM_USER_FILE",None)
                    else:os.environ["NTLM_USER_FILE"]=previous
        report["test_ca_added"]=trusted
        report["test_ca_removed"]=True if trusted else None
        print(json.dumps(report))

if __name__=="__main__":
    try:main()
    except Exception as error:
        print(json.dumps({"result":"FAIL","error_class":type(error).__name__,"raw_tokens_logged":False}))
        sys.exit(1)
