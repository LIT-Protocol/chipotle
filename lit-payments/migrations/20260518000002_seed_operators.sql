-- Seed the initial operator list. Emails are stored lowercased.
--
-- Adding/removing operators in the future: write a new migration. We
-- avoid an admin "manage operators" UI in v1 — the operator set is small
-- and rare to change.

INSERT INTO operators (email, role) VALUES
    ('chris@litprotocol.com',         'admin'),
    ('salamiademola73@gmail.com',     'mod')
ON CONFLICT (email) DO NOTHING;
