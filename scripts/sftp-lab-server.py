"""Confined POSIX SFTP simulation over real Paramiko; reusable by Windows labs."""
import errno
import os
from pathlib import PurePosixPath
import stat
import paramiko


class Files(paramiko.SFTPServerInterface):
    def __init__(self, server, root, **kwargs):
        super().__init__(server, **kwargs)
        self.root = root.resolve()
        self.state = server.case.state
        self.modes = {}

    def path(self, name):
        if "\\" in name or "\0" in name or ".." in PurePosixPath(name).parts:
            raise PermissionError(errno.EACCES, "Confined lab path")
        path = self.root.joinpath(*[p for p in PurePosixPath(name).parts if p not in ("/", ".")])
        if not path.resolve().is_relative_to(self.root) or path.is_symlink():
            raise PermissionError(errno.EACCES, "Confined lab path")
        return path

    def attributes(self, path):
        attributes = paramiko.SFTPAttributes.from_stat(path.stat())
        attributes.st_mode = (stat.S_IFDIR if path.is_dir() else stat.S_IFREG) | self.modes.get(path, 0o700 if path.is_dir() else 0o600)
        return attributes

    def canonicalize(self, name):
        try:
            relative = self.path(name).relative_to(self.root).as_posix()
            return "/" if relative == "." else "/" + relative
        except OSError:
            return "/"

    def list_folder(self, name):
        try:
            result = []
            for path in self.path(name).iterdir():
                item = self.attributes(path)
                item.filename = path.name
                result.append(item)
            return result
        except OSError as error:
            return paramiko.SFTPServer.convert_errno(error.errno)

    def stat(self, name):
        if name == "/blocked-link":
            item = paramiko.SFTPAttributes()
            item.st_mode = stat.S_IFLNK | 0o777
            item.st_size = 0
            return item
        try:
            return self.attributes(self.path(name))
        except OSError as error:
            return paramiko.SFTPServer.convert_errno(error.errno)
    lstat = stat

    def open(self, name, flags, attr):
        if name == "/blocked-link":
            return paramiko.SFTP_PERMISSION_DENIED
        try:
            path = self.path(name)
            descriptor = os.open(path, flags | getattr(os, "O_BINARY", 0), 0o600)
            mode = "r+b" if flags & os.O_RDWR else "wb" if flags & os.O_WRONLY else "rb"
            file = os.fdopen(descriptor, mode)
            if flags & os.O_CREAT:
                self.modes[path] = (attr.st_mode or 0o600) & 0o777
            return Handle(self, path, file, flags)
        except OSError as error:
            return paramiko.SFTPServer.convert_errno(error.errno)

    def mkdir(self, name, attr):
        try:
            path = self.path(name)
            path.mkdir()
            self.modes[path] = (attr.st_mode or 0o700) & 0o777
            return paramiko.SFTP_OK
        except OSError as error:
            return paramiko.SFTPServer.convert_errno(error.errno)

    def remove(self, name):
        try:
            path = self.path(name)
            if path.read_bytes() == b"edited via native SFTP" and self.modes.get(path) == 0o755:
                self.state["editor_mode_preserved"] = True
            path.unlink()
            self.modes.pop(path, None)
            return paramiko.SFTP_OK
        except OSError as error:
            return paramiko.SFTPServer.convert_errno(error.errno)

    def rmdir(self, name):
        try:
            path = self.path(name)
            path.rmdir()
            return paramiko.SFTP_OK
        except OSError as error:
            return paramiko.SFTPServer.convert_errno(error.errno)

    def rename(self, old, new):
        return self._rename(old, new, False)

    def posix_rename(self, old, new):
        return self._rename(old, new, True)

    def _rename(self, old, new, replace):
        try:
            source, destination = self.path(old), self.path(new)
            if destination.exists() and not replace:
                return paramiko.SFTP_FAILURE
            (os.replace if replace else os.rename)(source, destination)
            for path, mode in list(self.modes.items()):
                if path == source or source in path.parents:
                    self.modes[destination / path.relative_to(source)] = mode
                    del self.modes[path]
            return paramiko.SFTP_OK
        except OSError as error:
            return paramiko.SFTPServer.convert_errno(error.errno)

    def chattr(self, name, attr):
        try:
            path = self.path(name)
            if not path.exists():
                return paramiko.SFTP_NO_SUCH_FILE
            if attr.st_mode is not None:
                self.modes[path] = attr.st_mode & 0o777
            return paramiko.SFTP_OK
        except OSError as error:
            return paramiko.SFTPServer.convert_errno(error.errno)


class Handle(paramiko.SFTPHandle):
    def __init__(self, files, path, file, flags):
        super().__init__(flags)
        self.files, self.path = files, path
        self.readfile = self.writefile = file

    def stat(self):
        return self.files.attributes(self.path)

    def chattr(self, attr):
        return self.files.chattr("/" + self.path.relative_to(self.files.root).as_posix(), attr)
