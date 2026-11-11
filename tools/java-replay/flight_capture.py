"""Two-connection flight capture lifecycle; protocol/privacy checks remain in session.capture."""
import socket
import threading
from session import capture, stop
from tape import Writer
from wire import loopback


class FlightCapture:
    def __init__(self, path, listen, target):
        self.address, self.target = loopback(listen), loopback(target)
        if self.address == self.target:
            raise ValueError('capture listener must differ from actual server')
        self.listener = socket.create_server(self.address)
        self.listener.settimeout(0.25)
        try:
            self.writer = Writer(path)
        except BaseException:
            self.listener.close()
            raise
        self.stopping = threading.Event()
        self.lock = threading.Lock()
        self.active = []
        self.errors = []
        self.segments = 0
        self.worker = threading.Thread(target=self.run, name='flight-capture')
        self.worker.start()

    def own(self, stream):
        with self.lock:
            if self.stopping.is_set():
                stop(stream)
            else:
                self.active.append(stream)

    def run(self):
        try:
            for segment in range(2):
                while not self.stopping.is_set():
                    try:
                        client, _ = self.listener.accept()
                        break
                    except socket.timeout:
                        continue
                else:
                    return
                self.own(client)
                try:
                    with client:
                        capture(client, self.target, self.writer, segment,
                                self.address[1], server_opened=self.own)
                    self.segments += 1
                finally:
                    with self.lock:
                        self.active.clear()
        except Exception as error:
            if not self.stopping.is_set():
                self.errors.append(str(error))
        finally:
            self.listener.close()

    def finish(self, success):
        if success:
            # The caller first disconnects the real client. Never force a successful segment.
            self.worker.join(timeout=15)
        self.stopping.set()
        self.listener.close()
        with self.lock:
            for stream in self.active:
                stop(stream)
        self.worker.join(timeout=20)
        completed = success and not self.worker.is_alive() and not self.errors and self.segments == 2
        if self.worker.is_alive():
            # Do not close a writer while an owned pump can still append to it.
            raise ValueError('capture worker did not retire; tape remains incomplete')
        if completed:
            self.writer.close()
        else:
            self.writer.abort()
        result = {'complete': completed, 'segments': self.segments, 'errors': self.errors}
        return result
