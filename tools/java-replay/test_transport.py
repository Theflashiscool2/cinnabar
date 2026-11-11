"""Native TCP policy checks with owned mock sockets; no network/client/server launch."""
import socket
import unittest
from unittest.mock import MagicMock, patch
import session
from wire import tcp_nodelay


class NativeTransportTests(unittest.TestCase):
    def test_tcp_policy_matches_native_and_leaves_unix_streams_untouched(self):
        for family in [socket.AF_INET, socket.AF_INET6, socket.AF_UNIX]:
            stream = MagicMock()
            stream.family = family
            tcp_nodelay(stream)
            if family == socket.AF_UNIX:
                stream.setsockopt.assert_not_called()
            else:
                stream.setsockopt.assert_called_once_with(socket.IPPROTO_TCP, socket.TCP_NODELAY, 1)

    def test_capture_configures_accepted_and_upstream_tcp_before_forwarding(self):
        client, server = MagicMock(), MagicMock()
        client.family = server.family = socket.AF_INET
        server.__enter__.return_value = server
        with patch('session.login', return_value=(b'', b'')), \
                patch('session.socket.create_connection', return_value=server), \
                patch('session._capture') as forward:
            session.capture(client, ('127.0.0.1', 25586), None, 0, 25587)
            forward.assert_called_once()
        for stream in [client, server]:
            stream.setsockopt.assert_called_once_with(socket.IPPROTO_TCP, socket.TCP_NODELAY, 1)

    def test_replay_configures_accepted_tcp_before_login(self):
        client = MagicMock()
        client.family = socket.AF_INET
        with patch('session.login', side_effect=ValueError('stop before protocol')):
            with self.assertRaisesRegex(ValueError, 'stop before protocol'):
                session.replay(client, {}, 25587, 1)
        client.setsockopt.assert_called_once_with(socket.IPPROTO_TCP, socket.TCP_NODELAY, 1)


if __name__ == '__main__':
    unittest.main()
