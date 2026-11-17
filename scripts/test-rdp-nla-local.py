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
from spnego.exceptions import SpnegoError
from spnego._negotiate import NegotiateProxy
from spnego._ntlm_raw.crypto import ntowfv1
from spnego.tls import CredSSPTLSContext, get_certificate_public_key

logging.getLogger("spnego").addHandler(logging.NullHandler())
logging.getLogger("spnego").propagate = False
os.umask(0o077)
USER, DOMAIN = "rr-lab", "RRTEST"
CURRENT_STAGE="initialization"
FAILURE_STAGE=None


class LabNegotiate(NegotiateProxy):
    last_error=None

    def step(self, *args, **kwargs):
        try:
            return super().step(*args, **kwargs)
        except SpnegoError as error:
            # CredSSP can encode this exception in TSRequest instead of raising
            # until its next step. Retain only type/status, never message/token.
            self.last_error={"error_class":type(error).__name__,"status":error.nt_status}
            raise

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


def powershell(script, root, thumb="", extra=None):
    path=Path(os.environ.get("RUSTREMOTE_LAB_PWSH",str(Path(os.environ.get("SystemRoot","C:/Windows"))/"System32/WindowsPowerShell/v1.0/powershell.exe")))
    if not path.is_absolute() or not path.is_file(): raise RuntimeError("Trusted PowerShell unavailable")
    result=subprocess.run([str(path),"-NoProfile","-NonInteractive","-Command",script],
                          env=dict(os.environ,RUSTREMOTE_NLA_ROOT=str(root),RUSTREMOTE_NLA_THUMB=thumb,**(extra or {})),
                          capture_output=True,text=True,timeout=30)
    if result.returncode==17: raise ExistingLabCertificate("Fixture certificate already existed")
    if result.returncode: raise RuntimeError("Private laboratory setup or certificate cleanup failed")
    return result.stdout


def secure_folder(root):
    powershell("""$ErrorActionPreference='Stop'
$sid=[Security.Principal.WindowsIdentity]::GetCurrent().User
$acl=[Security.AccessControl.DirectorySecurity]::new()
$acl.SetOwner($sid); $acl.SetAccessRuleProtection($true,$false)
foreach($who in @($sid,[Security.Principal.SecurityIdentifier]::new('S-1-5-18'))){
$acl.AddAccessRule([Security.AccessControl.FileSystemAccessRule]::new($who,'FullControl','ContainerInherit,ObjectInherit','None','Allow'))}
Set-Acl -LiteralPath $env:RUSTREMOTE_NLA_ROOT -AclObject $acl
""",root)


def approve_fixture_certificate(pid, thumb, subject):
    """Approve only our CI certutil's exact generated public certificate, never an RDP warning."""
    if os.environ.get("GITHUB_ACTIONS")!="true":return False
    import ctypes, re
    from ctypes import wintypes as w
    user=ctypes.WinDLL("user32",use_last_error=True)
    callback=ctypes.WINFUNCTYPE(w.BOOL,w.HWND,w.LPARAM)
    user.EnumWindows.argtypes=[callback,w.LPARAM]
    user.EnumChildWindows.argtypes=[w.HWND,callback,w.LPARAM]
    user.GetWindowThreadProcessId.argtypes=[w.HWND,ctypes.POINTER(w.DWORD)]
    user.GetClassNameW.argtypes=[w.HWND,w.LPWSTR,ctypes.c_int]
    user.GetDlgCtrlID.argtypes=[w.HWND]
    user.SendMessageTimeoutW.argtypes=[w.HWND,w.UINT,w.WPARAM,w.LPARAM,w.UINT,w.UINT,ctypes.POINTER(ctypes.c_size_t)]
    approved=False
    @callback
    def top(window,unused):
        nonlocal approved
        owner=w.DWORD();user.GetWindowThreadProcessId(window,ctypes.byref(owner))
        if owner.value!=pid:return True
        texts=[];buttons=[];visited=0
        @callback
        def child(control,unused):
            nonlocal visited
            visited+=1
            if visited>128:return False
            name=ctypes.create_unicode_buffer(64);user.GetClassNameW(control,name,len(name))
            if name.value not in ("Static","Button"):return True
            text=ctypes.create_unicode_buffer(4096);length=ctypes.c_size_t()
            user.SendMessageTimeoutW(control,0x000D,len(text),ctypes.cast(text,ctypes.c_void_p).value,3,50,ctypes.byref(length))
            texts.append(text.value)
            if name.value=="Button" and user.GetDlgCtrlID(control)==6:buttons.append(control)
            return True
        user.EnumChildWindows(window,child,0)
        text="\n".join(texts)
        if subject in text and thumb in re.sub("[^0-9A-F]","",text.upper()) and len(buttons)==1:
            result=ctypes.c_size_t()
            user.SendMessageTimeoutW(buttons[0],0x00F5,0,0,3,100,ctypes.byref(result))
            approved=True
        return True
    user.EnumWindows(top,0)
    return approved


def certificate_command(command, root, thumb):
    subject=x509.load_der_x509_certificate((root/"ca.cer").read_bytes()).subject.get_attributes_for_oid(NameOID.COMMON_NAME)[0].value
    if not subject.startswith("RustRemote loopback lab "):raise RuntimeError("Unexpected fixture certificate subject")
    with subprocess.Popen(command,stdout=subprocess.PIPE,stderr=subprocess.PIPE) as process:
        deadline=time.monotonic()+20
        while True:
            try:
                stdout,stderr=process.communicate(timeout=.25)
                return subprocess.CompletedProcess(command,process.returncode,stdout,stderr)
            except subprocess.TimeoutExpired:
                approve_fixture_certificate(process.pid,thumb,subject)
                if time.monotonic()>deadline:
                    process.kill();process.communicate()
                    raise TimeoutError("Owned fixture certificate confirmation did not complete")


def trust(root, thumb, add):
    # certutil's explicit force operation avoids .NET's interactive protected-root prompt.
    # Precheck the exact unique fixture certificate; never force over an existing entry.
    precheck="""$ErrorActionPreference='Stop'
$certificate=[Security.Cryptography.X509Certificates.X509Certificate2]::new([IO.File]::ReadAllBytes((Join-Path $env:RUSTREMOTE_NLA_ROOT 'ca.cer')))
if($certificate.Thumbprint -ne $env:RUSTREMOTE_NLA_THUMB){throw 'Fixture identity mismatch'}
$store=[Security.Cryptography.X509Certificates.X509Store]::new('Root','CurrentUser')
$store.Open([Security.Cryptography.X509Certificates.OpenFlags]::ReadOnly)
try {
$count=$store.Certificates.Find([Security.Cryptography.X509Certificates.X509FindType]::FindByThumbprint,$env:RUSTREMOTE_NLA_THUMB,$false).Count
__CHECK__
} finally {$store.Close()}
"""
    powershell(precheck.replace("__CHECK__","if($count -ne 0){exit 17}" if add else ""),root,thumb)
    import ctypes
    directory=ctypes.create_unicode_buffer(32768)
    size=ctypes.windll.kernel32.GetSystemDirectoryW(directory,len(directory))
    if not 0<size<len(directory):raise RuntimeError("Windows system directory unavailable")
    command=[str(Path(directory.value)/"certutil.exe"),"-user","-f","-addstore" if add else "-delstore","Root",str(root/"ca.cer") if add else thumb]
    result=certificate_command(command,root,thumb)
    # Removal may find nothing after an add operation failed; verify state independently.
    if add and result.returncode:raise RuntimeError("Fixture CA import failed")
    powershell(precheck.replace("__CHECK__","if($count -ne "+("1" if add else "0")+"){throw 'Fixture trust state mismatch'}"),root,thumb)


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
        self.state={"phase":"accept","negotiation":False,"credssp_complete":False,"password_match":False,"account_match":False,
                    "credssp_records":0,"credssp_responses":0,"credssp_record_types":{}}
        self.worker=threading.Thread(target=self.serve);self.worker.start()

    def serve(self):
        try:
            with self.listener.accept()[0] as stream:
                stream.settimeout(15)
                self.state["phase"]="rdp-negotiation"
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
                authentication=negotiation("accept")
                context=spnego.server(protocol="credssp",credssp_negotiate_context=authentication,
                                      credssp_tls_context=self.context,credssp_min_protocol=5)
                self.state["phase"]="tls"
                tls_handshake(stream,context)
                self.state["tls_version"]=context.get_extra_info("ssl_object").version()
                self.state["phase"]="credssp"
                initial=context.step()
                if initial:
                    stream.sendall(initial)
                    self.state["credssp_responses"]+=1
                budget=0
                for _ in range(64):
                    header=receive(stream,5);length=int.from_bytes(header[3:],"big")
                    budget+=length+5
                    if header[0] not in (20,21,22,23) or length>18432 or budget>256*1024: raise ValueError()
                    self.state["credssp_records"]+=1
                    kinds=self.state["credssp_record_types"]
                    kinds[str(header[0])]=kinds.get(str(header[0]),0)+1
                    token=context.step(header+receive(stream,length))
                    self.state["auth_stage"]=context.get_extra_info("auth_stage")
                    self.state["credssp_version"]=context.get_extra_info("protocol_version")
                    self.state["ntlm_complete"]=authentication.complete
                    self.state["mechlist_mic_sent"]=authentication._mic_sent
                    self.state["mechlist_mic_received"]=authentication._mic_recv
                    if authentication.last_error:self.state["authentication_error"]=authentication.last_error
                    protocol=context.negotiated_protocol
                    if protocol in ("ntlm","kerberos"):self.state["auth_protocol"]=protocol
                    if token:
                        stream.sendall(token)
                        self.state["credssp_responses"]+=1
                    if context.complete:
                        self.state["phase"]="delegated-password"
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


class LoopbackClientTLS(ssl.SSLContext):
    def wrap_bio(self, incoming, outgoing, server_side=False, server_hostname=None, session=None):
        # pyspnego's BIO adapter omits server_hostname. Supply the fixture's
        # literal identity so the normal TLS verifier also checks its IP SAN.
        if server_side:raise ValueError("Client-only loopback TLS context")
        return super().wrap_bio(incoming,outgoing,server_side=False,
                                server_hostname=server_hostname or "127.0.0.1",session=session)


def verify_fixture(peer, root, password, native_sspi=False):
    context=LoopbackClientTLS(ssl.PROTOCOL_TLS_CLIENT)
    context.minimum_version=ssl.TLSVersion.TLSv1_2;context.load_verify_locations(root/"ca.pem")
    authentication=(spnego.client(DOMAIN+"\\"+USER,password,protocol="ntlm",hostname="127.0.0.1",service="TERMSRV",
                                  options=spnego.NegotiateOptions.use_sspi) if native_sspi else negotiation("initiate",password))
    client=spnego.client(DOMAIN+"\\"+USER,password,protocol="credssp",credssp_negotiate_context=authentication,
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


def confirm_fixture_file_dialogs(parent, root, port):
    """Simulate user file-opening consent on a disposable CI account only.

    Never alter policy, trust a TLS warning, enter a password or enable redirection.
    """
    if os.environ.get("GITHUB_ACTIONS")!="true" or not 1024<=port<=65535:
        raise RuntimeError("RDP file consent is restricted to the loopback CI fixture")
    script=r"""$ErrorActionPreference='Stop'
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName System.Drawing
Add-Type 'using System; using System.Text; using System.Runtime.InteropServices; public static class IntroCapture { [StructLayout(LayoutKind.Sequential)] public struct Rect {public int Left,Top,Right,Bottom;} [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr window,out Rect rect); [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr window,IntPtr dc,uint flags); [DllImport("user32.dll")] public static extern IntPtr GetDlgItem(IntPtr window,int id); [DllImport("user32.dll",CharSet=CharSet.Unicode)] public static extern int GetWindowTextW(IntPtr window,StringBuilder text,int size); [DllImport("user32.dll")] public static extern bool IsWindowEnabled(IntPtr window); [DllImport("user32.dll")] public static extern bool PostMessageW(IntPtr window,uint message,UIntPtr param,IntPtr data); }'
function Label($control){return (($control.Current.Name -replace '&','' -replace '\u2019',"'" -replace '\s+',' ').Trim())}
function ClickVerifiedTaskButton($window,$button){
if(-not $button.Current.IsEnabled){throw 'Verified fixture button is disabled'}
if($button.GetCurrentPropertyValue([Windows.Automation.AutomationElement]::IsInvokePatternAvailableProperty)){
$button.GetCurrentPattern([Windows.Automation.InvokePattern]::Pattern).Invoke();return
}
if($window.Current.ClassName -ne '#32770'){throw 'Unknown fixture dialog class'}
if((Label $button) -ceq 'Connect'){
# The legacy publisher warning has a native IDOK button instead of a TaskDialog.
$native=[IntroCapture]::GetDlgItem([IntPtr]$window.Current.NativeWindowHandle,1)
$text=[Text.StringBuilder]::new(64)
if($native -ne [IntPtr]::Zero -and [IntroCapture]::IsWindowEnabled($native)){
[void][IntroCapture]::GetWindowTextW($native,$text,64)
if(($text.ToString() -replace '&','').Trim() -ceq 'Connect'){
if(-not [IntroCapture]::PostMessageW($native,245,[UIntPtr]::Zero,[IntPtr]::Zero)){throw 'Fixture legacy Connect action not queued'}
return
}
}
}
if($button.Current.AutomationId -cnotmatch '^CommandButton_([0-9]{1,5})$'){throw 'Unknown fixture task-dialog button'}
$id=[uint32]$Matches[1]
if($id -gt 65535){throw 'Invalid fixture task-dialog button ID'}
# TDM_CLICK_BUTTON = WM_USER + 102. The caller has verified the entire fixture
# dialog; this is never called for certificate or credential warnings.
# Queue the click: its callback may open another modal dialog.
if(-not [IntroCapture]::PostMessageW([IntPtr]$window.Current.NativeWindowHandle,1126,[UIntPtr]$id,[IntPtr]::Zero)){throw 'Fixture task-dialog action not queued'}
}
$children=@(Get-CimInstance Win32_Process -Filter ("ParentProcessId="+$env:RUSTREMOTE_NLA_PARENT+" AND Name='mstsc.exe'"))
$intro=0;$connection=0;$candidate=@{};$connectionCandidate=@{};$deadline=[DateTime]::UtcNow.AddSeconds(12)
while([DateTime]::UtcNow -lt $deadline){
foreach($process in $children){
$condition=[Windows.Automation.PropertyCondition]::new([Windows.Automation.AutomationElement]::ProcessIdProperty,[int]$process.ProcessId)
$windows=[Windows.Automation.AutomationElement]::RootElement.FindAll([Windows.Automation.TreeScope]::Children,$condition)
foreach($window in $windows){
$controls=@($window.FindAll([Windows.Automation.TreeScope]::Descendants,[Windows.Automation.Condition]::TrueCondition))
if(@($controls|Where-Object{$_.Current.IsPassword}).Count){continue}
$boxes=@($controls|Where-Object{$_.Current.ControlType -eq [Windows.Automation.ControlType]::CheckBox})
$introLabels=@($controls|Where-Object{(Label $_) -cmatch '^I understand and allow (RDP|\.rdp) files to open on this device for my account$'})
$introTitle=@($controls|Where-Object{(Label $_) -ceq 'Opening Remote Desktop Connection'})
$ok=@($controls|Where-Object{(Label $_) -ceq 'OK' -and $_.Current.AutomationId -ceq 'CommandButton_1'})
$candidate=@{checkboxes=$boxes.Count;intro_labels=$introLabels.Count;intro_titles=$introTitle.Count;ok_buttons=$ok.Count}
if($intro -eq 0 -and $boxes.Count -eq 1 -and $introLabels.Count -ge 1 -and $introTitle.Count -eq 1){
# Capture only this proven educational dialog, which contains no account or
# password fields. Never capture credential, certificate or arbitrary windows.
$capture=Join-Path $env:RUSTREMOTE_NLA_ROOT 'rdp-intro.png'
if(-not (Test-Path -LiteralPath $capture)){
$rect=[IntroCapture+Rect]::new();$handle=[IntPtr]$window.Current.NativeWindowHandle
if([IntroCapture]::GetWindowRect($handle,[ref]$rect) -and ($rect.Right-$rect.Left) -gt 0 -and ($rect.Right-$rect.Left) -le 4096 -and ($rect.Bottom-$rect.Top) -gt 0 -and ($rect.Bottom-$rect.Top) -le 4096){
$bitmap=[Drawing.Bitmap]::new(($rect.Right-$rect.Left),($rect.Bottom-$rect.Top))
try{$graphics=[Drawing.Graphics]::FromImage($bitmap)
try{$dc=$graphics.GetHdc();try{$drawn=[IntroCapture]::PrintWindow($handle,$dc,2)}finally{$graphics.ReleaseHdc($dc)}}finally{$graphics.Dispose()}
if($drawn){$bitmap.Save($capture,[Drawing.Imaging.ImageFormat]::Png)}
}finally{$bitmap.Dispose()}
}
}
$toggle=$boxes[0].GetCurrentPattern([Windows.Automation.TogglePattern]::Pattern)
if($toggle.Current.ToggleState -eq [Windows.Automation.ToggleState]::Off){$toggle.Toggle()}
if($toggle.Current.ToggleState -ne [Windows.Automation.ToggleState]::On){throw 'Fixture opening consent not selected'}
# Only this already identified, non-credential dialog may expose label metadata.
# Record only this verified introduction, never a credential dialog.
$debug=Join-Path $env:RUSTREMOTE_NLA_ROOT 'rdp-intro-accessibility.json'
if(-not (Test-Path -LiteralPath $debug)){
@($controls|Select-Object -First 128|ForEach-Object{
@{type=$_.Current.ControlType.ProgrammaticName;name=(Label $_);id=$_.Current.AutomationId;enabled=$_.Current.IsEnabled;invoke=$_.GetCurrentPropertyValue([Windows.Automation.AutomationElement]::IsInvokePatternAvailableProperty)}
})|ConvertTo-Json -Depth 3|Set-Content -LiteralPath $debug -Encoding utf8
}
if($ok.Count -eq 1 -and $ok[0].Current.IsEnabled){
ClickVerifiedTaskButton $window $ok[0]
$intro++;continue
}
}
$labels=@($controls|Where-Object{-not $_.Current.IsPassword}|ForEach-Object{Label $_})
$connect=@($controls|Where-Object{(Label $_) -ceq 'Connect' -and $_.Current.IsEnabled})
$publisherField=($labels -ccontains 'Publisher:' -or $labels -ccontains 'Publisher')
$computerField=($labels -ccontains 'Remote computer:' -or $labels -ccontains 'Remote computer')
$legacy=($publisherField -and $computerField)
$banner=($labels -ccontains 'Caution: Unknown remote connection')
# The actor's fixed profile and private file carry the listener's exact port.
# A read-only destination field may expose its text as a value, not its name.
$target=($labels -ccontains $env:RUSTREMOTE_NLA_TARGET -or ($legacy -and $labels -ccontains '127.0.0.1'))
if(-not $target -and ($banner -or $legacy) -and $labels -ccontains 'Unknown publisher'){
foreach($control in $controls){
if(-not $control.Current.IsPassword -and $control.GetCurrentPropertyValue([Windows.Automation.AutomationElement]::IsValuePatternAvailableProperty)){
$value=$control.GetCurrentPattern([Windows.Automation.ValuePattern]::Pattern)
if($value.Current.IsReadOnly -and ($value.Current.Value -ceq $env:RUSTREMOTE_NLA_TARGET -or ($legacy -and $value.Current.Value -ceq '127.0.0.1'))){$target=$true}
}
}
}
$connectionCandidate=@{banner=$banner;legacy=$legacy;publisher_field=$publisherField;computer_field=$computerField;publisher=($labels -ccontains 'Unknown publisher');target=$target;connect=$connect.Count}
if($connection -eq 0 -and ($banner -or $legacy) -and $labels -ccontains 'Unknown publisher' -and $target -and $connect.Count -eq 1){
foreach($box in $boxes){
if($box.GetCurrentPattern([Windows.Automation.TogglePattern]::Pattern).Current.ToggleState -ne [Windows.Automation.ToggleState]::Off){throw 'Fixture unexpectedly requests local resources'}
}
ClickVerifiedTaskButton $window $connect[0];$connection++
}
}}
if($connection -eq 1){break}
Start-Sleep -Milliseconds 200
}
@{intro_confirmed=$intro;loopback_file_confirmed=$connection;intro_candidate=$candidate;connection_candidate=$connectionCandidate;client_pids=@($children|ForEach-Object{$_.ProcessId})}|ConvertTo-Json -Compress
"""
    script="try {\n"+script+r"""
} catch {
@{error_type=$_.Exception.GetType().FullName;error_id=$_.FullyQualifiedErrorId}|ConvertTo-Json -Compress|Set-Content -LiteralPath (Join-Path $env:RUSTREMOTE_NLA_ROOT 'rdp-consent-error.json') -Encoding utf8
exit 1
}
"""
    try:
        try:
            result=json.loads(powershell(script,root,extra={"RUSTREMOTE_NLA_PARENT":str(parent),"RUSTREMOTE_NLA_TARGET":f"127.0.0.1:{port}"}))
        except RuntimeError:
            error=root/"rdp-consent-error.json"
            if not error.is_file() or error.stat().st_size>4096:raise
            result=json.loads(error.read_text(encoding="utf-8-sig"))
    finally:
        # Keep the verified introduction evidence even if its action fails.
        for name in ("rdp-intro.png","rdp-intro-accessibility.json","rdp-consent-error.json"):
            capture=root/name
            if capture.is_file():
                report=Path("dist/windows-probe")/name
                with report.open("xb") as output:output.write(capture.read_bytes())
    return result


def native_client_events(pids, started, root):
    """Read only numeric RDP client diagnostics for the fixture's exact processes."""
    script=r"""$ErrorActionPreference='Stop'
$pids=@($env:RUSTREMOTE_NLA_PIDS.Split(',')|Where-Object{$_}|ForEach-Object{[uint32]$_})
$start=[DateTimeOffset]::Parse($env:RUSTREMOTE_NLA_STARTED).LocalDateTime
$events=@();$channels=@()
foreach($channel in @('Microsoft-Windows-TerminalServices-RDPClient/Operational','Microsoft-Windows-TerminalServices-ClientActiveXCore/Operational')){
try{
$records=@(Get-WinEvent -FilterHashtable @{LogName=$channel;StartTime=$start} -MaxEvents 64 -ErrorAction Stop)
$channels+=@{channel=$channel;state='read'}
foreach($record in $records){
$xml=[xml]$record.ToXml()
if($pids -notcontains [uint32]$xml.Event.System.Execution.ProcessID){continue}
$codes=@{}
foreach($field in $xml.SelectNodes('//*[local-name()="EventData"]/* | //*[local-name()="UserData"]//*[not(*)]')){
$name=if($field.HasAttribute('Name')){$field.GetAttribute('Name')}else{$field.LocalName}
$value=[string]$field.InnerText
if($name -in @('ErrorCode','Reason','ReasonCode','DisconnectReason','ExtendedReason','Status','HResult','ResultCode','Error','Code') -and $value -match '^(0x[0-9a-fA-F]{1,16}|-?[0-9]{1,20})$'){$codes[$name]=$value}
}
$events+=@{channel=$channel;id=$record.Id;codes=$codes}
}
}catch{$channels+=@{channel=$channel;state='empty_or_unavailable'}}
}
@{channels=$channels;events=$events}|ConvertTo-Json -Depth 5 -Compress
"""
    return json.loads(powershell(script,root,extra={"RUSTREMOTE_NLA_PIDS":",".join(str(int(pid)) for pid in pids),"RUSTREMOTE_NLA_STARTED":started}))


def native_window_flags(parent, root):
    script=r"""$ErrorActionPreference='Stop'
Add-Type -AssemblyName UIAutomationClient
Add-Type 'using System; using System.Runtime.InteropServices; public static class OwnedPopup { [DllImport("user32.dll")] public static extern IntPtr GetLastActivePopup(IntPtr window); }'
$children=@(Get-CimInstance Win32_Process -Filter ("ParentProcessId="+$env:RUSTREMOTE_NLA_PARENT+" AND Name='mstsc.exe'"))
$flags=0
foreach($process in $children){
$condition=[Windows.Automation.PropertyCondition]::new([Windows.Automation.AutomationElement]::ProcessIdProperty,[int]$process.ProcessId)
$windows=@([Windows.Automation.AutomationElement]::RootElement.FindAll([Windows.Automation.TreeScope]::Children,$condition))
# A credential broker can run in another process. Only follow a popup owned by
# a proven mstsc window; never search unrelated security windows.
foreach($window in @($windows)){
$handle=[IntPtr]$window.Current.NativeWindowHandle
$popup=[OwnedPopup]::GetLastActivePopup($handle)
if($popup -ne [IntPtr]::Zero -and $popup -ne $handle){$windows+=,[Windows.Automation.AutomationElement]::FromHandle($popup)}
}
foreach($window in $windows){
$flags=$flags -bor 1
$controls=@($window)+@($window.FindAll([Windows.Automation.TreeScope]::Descendants,[Windows.Automation.Condition]::TrueCondition))
foreach($control in $controls){
if($control.Current.IsPassword){$flags=$flags -bor 2;continue}
if($control.Current.Name -match 'phishing|RDP files|Remote Desktop files'){$flags=$flags -bor 64}
if($control.Current.ControlType -eq [Windows.Automation.ControlType]::Button -and $control.Current.Name -match '^Allow$|^Continue$'){$flags=$flags -bor 128}
if($control.Current.ControlType -eq [Windows.Automation.ControlType]::Button -and $control.Current.Name -match 'Connect'){$flags=$flags -bor 4}
if($control.Current.ControlType -in @([Windows.Automation.ControlType]::Text,[Windows.Automation.ControlType]::Window,[Windows.Automation.ControlType]::Pane)){
if($control.Current.Name -match 'credential|password'){$flags=$flags -bor 8}
if($control.Current.Name -match 'invalid|corrupt|connection file'){$flags=$flags -bor 16}}
if($control.Current.ControlType -eq [Windows.Automation.ControlType]::Text -and $control.Current.Name -match 'publisher|trust|identify'){$flags=$flags -bor 32}
}}
}
$policy=(Get-ItemProperty -LiteralPath 'HKLM:\SOFTWARE\Policies\Microsoft\Windows NT\Terminal Services' -Name DisablePasswordSaving -ErrorAction SilentlyContinue).DisablePasswordSaving
@{owned_client_count=$children.Count;ui_flags=$flags;disable_password_saving=$policy;pids=@($children|ForEach-Object{$_.ProcessId})}|ConvertTo-Json -Compress
"""
    result=json.loads(powershell(script,root,extra={"RUSTREMOTE_NLA_PARENT":str(parent)}))
    pids=set(result.pop("pids"))
    # UIA can omit legacy mstsc dialogs. Inspect only Static labels in owned windows,
    # publishing fixed categories and numeric button IDs, never their text/values.
    import ctypes
    from ctypes import wintypes
    user=ctypes.WinDLL("user32",use_last_error=True)
    callback=ctypes.WINFUNCTYPE(wintypes.BOOL,wintypes.HWND,wintypes.LPARAM)
    user.EnumWindows.argtypes=[callback,wintypes.LPARAM]
    user.EnumChildWindows.argtypes=[wintypes.HWND,callback,wintypes.LPARAM]
    user.GetWindowThreadProcessId.argtypes=[wintypes.HWND,ctypes.POINTER(wintypes.DWORD)]
    user.GetClassNameW.argtypes=[wintypes.HWND,wintypes.LPWSTR,ctypes.c_int]
    user.GetDlgCtrlID.argtypes=[wintypes.HWND]
    user.IsWindowVisible.argtypes=[wintypes.HWND]
    user.SendMessageTimeoutW.argtypes=[wintypes.HWND,wintypes.UINT,ctypes.c_size_t,ctypes.c_ssize_t,wintypes.UINT,wintypes.UINT,ctypes.POINTER(ctypes.c_size_t)]
    categories=set();buttons=set();classes=set();windows=[]
    words=("publisher","trust","credential","password","connect","error","invalid","identify","identity","security","certificate","remote","desktop","session","license","policy","authentication","version","computer","blocked","phishing","rdp files","allow","continue")
    def classify_label(control):
        text=ctypes.create_unicode_buffer(8192);length=ctypes.c_size_t()
        user.SendMessageTimeoutW(control,0x000D,len(text),ctypes.cast(text,ctypes.c_void_p).value,3,50,ctypes.byref(length))
        categories.update(word for word in words if word in text.value.lower())
    @callback
    def top(window,unused):
        pid=wintypes.DWORD();user.GetWindowThreadProcessId(window,ctypes.byref(pid))
        if pid.value not in pids:return True
        name=ctypes.create_unicode_buffer(128);user.GetClassNameW(window,name,len(name))
        # Window class and visibility contain no user input or credential values.
        windows.append(dict(window_class=name.value,visible=bool(user.IsWindowVisible(window))))
        classify_label(window)
        visited=0
        @callback
        def child(control,unused):
            nonlocal visited
            visited+=1
            if visited>128:return False
            name=ctypes.create_unicode_buffer(128);user.GetClassNameW(control,name,len(name))
            classes.add(name.value)
            if name.value=="Button":buttons.add(user.GetDlgCtrlID(control))
            # Only labels; never WM_GETTEXT on Edit, ComboBox, browser or custom
            # credential controls, even when accessibility metadata is absent.
            if name.value in ("Static","Button","SysLink"):classify_label(control)
            return True
        user.EnumChildWindows(window,child,0)
        return True
    user.EnumWindows(top,0)
    result.update(static_categories=sorted(categories),button_ids=sorted(buttons),control_classes=sorted(classes),windows=windows)
    return result


def main():
    global CURRENT_STAGE, FAILURE_STAGE
    parser=argparse.ArgumentParser();mode=parser.add_mutually_exclusive_group(required=True)
    mode.add_argument("--verify-fixture",action="store_true");mode.add_argument("--verify-sspi-fixture",action="store_true");mode.add_argument("--native",type=Path)
    parser.add_argument("--fetch-deps",action="store_true");args=parser.parse_args()
    if not args.verify_fixture and not (sys.platform=="win32" and os.environ.get("GITHUB_ACTIONS")=="true"):
        parser.error("Native certificate changes are restricted to an ephemeral GitHub Windows runner")
    trusted=False;peer=None
    with tempfile.TemporaryDirectory(prefix="RustRemote NLA ") as folder:
        root=Path(folder)
        CURRENT_STAGE="secure-folder"
        if sys.platform=="win32":secure_folder(root)
        password=secrets.token_urlsafe(24)+"é"
        user_file=root/"ntlm-cache"
        user_file.write_text(f"{DOMAIN}\\{USER}:0:{'0'*32}:{ntowfv1(password).hex()}:[U]:LCT-0\n",encoding="utf-8")
        previous=os.environ.get("NTLM_USER_FILE");os.environ["NTLM_USER_FILE"]=str(user_file)
        try:
            CURRENT_STAGE="certificate-generation"
            context,thumb=certificate(root)
            if args.native:
                CURRENT_STAGE="trust-add"
                trusted=True
                try:trust(root,thumb,True)
                except ExistingLabCertificate:
                    trusted=False
                    raise
            CURRENT_STAGE="peer-start"
            peer=Peer(context,root,password)
            if args.verify_fixture or args.verify_sspi_fixture:verify_fixture(peer,root,password,native_sspi=args.verify_sspi_fixture)
            else:
                profile=dict(name="CredSSP loopback lab",host="127.0.0.1",port=peer.port,username=USER,favorite=False,
                    options=dict(protocol="Rdp",domain=DOMAIN,width=1280,height=720,fullscreen=False,clipboard=False,clipboard_files=False))
                CURRENT_STAGE="native-client"
                started=datetime.datetime.now(datetime.timezone.utc).isoformat()
                command=[str(args.native.resolve(strict=True)),"--self-test-rdp-nla"]
                with subprocess.Popen(command,stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True,encoding="utf-8") as process:
                    try:
                        try:
                            stdout,stderr=process.communicate(json.dumps(dict(profile=profile,secret=password,state=str(root/"rdp-nla-state.json"))),timeout=5)
                        except subprocess.TimeoutExpired:
                            peer.state["file_consents"]=confirm_fixture_file_dialogs(process.pid,root,peer.port)
                            if "error_type" in peer.state["file_consents"]:raise RuntimeError("Native fixture consent action failed")
                            try:peer.state["native_ui"]=native_window_flags(process.pid,root)
                            except Exception as error:peer.state["ui_probe_error"]=type(error).__name__
                            stdout,stderr=process.communicate(timeout=35)
                    except subprocess.TimeoutExpired:
                        # Popen.__exit__ waits indefinitely: reap our hung actor before
                        # unwinding so the fixture's certificate cleanup still runs.
                        process.kill();process.communicate()
                        raise TimeoutError("Native RDP lab actor exceeded its deadline") from None
                    result=subprocess.CompletedProcess(command,process.returncode,stdout,stderr)
                peer.state["native_exit"]=result.returncode
                try:peer.state["client_events"]=native_client_events(peer.state.get("file_consents",{}).get("client_pids",[]),started,root)
                except Exception as error:peer.state["event_probe_error"]=type(error).__name__
                if result.returncode:raise RuntimeError("Native RDP lab client did not complete")
            peer.close()
            CURRENT_STAGE="credential-verification"
            if not all(peer.state.get(key) for key in ("negotiation","credssp_complete","account_match","password_match")):
                raise RuntimeError("Native RDP profile credential was not authenticated")
            report=dict(result="PASS",target="127.0.0.1",client="native_sspi" if args.verify_sspi_fixture else "pyspnego" if args.verify_fixture else "mstsc",
                        real_windows_account=False,desktop_session=False,**peer.state)
        except Exception:
            FAILURE_STAGE=CURRENT_STAGE
            if peer: print(json.dumps({"stage":"CredSSP", "events":peer.state}))
            raise
        finally:
            try:
                if peer and peer.worker.is_alive():peer.close()
            finally:
                try:
                    if trusted:
                        CURRENT_STAGE="trust-remove"
                        trust(root,thumb,False)
                finally:
                    if previous is None:os.environ.pop("NTLM_USER_FILE",None)
                    else:os.environ["NTLM_USER_FILE"]=previous
        report["test_ca_added"]=trusted
        report["test_ca_removed"]=True if trusted else None
        print(json.dumps(report))

if __name__=="__main__":
    try:main()
    except Exception as error:
        print(json.dumps({"result":"FAIL","stage":FAILURE_STAGE or CURRENT_STAGE,"error_class":type(error).__name__,"raw_tokens_logged":False}))
        sys.exit(1)
