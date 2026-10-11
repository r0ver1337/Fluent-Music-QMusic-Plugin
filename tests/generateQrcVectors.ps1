param([string]$OutputPath = (Join-Path $PSScriptRoot 'fixtures/qrcVectors.json'))
$ErrorActionPreference = 'Stop'
$revision = '0e1494194523dd885405812a91ee9b9702bfb30c'
$sourceUrl = "https://raw.githubusercontent.com/WXRIW/QQMusicDecoder/$revision/QQMusicDecoder/DESHelper.cs"
$source = (Invoke-WebRequest -UseBasicParsing -Uri $sourceUrl).Content
# Only adapt C# range syntax for Windows PowerShell's older compiler.
$source = $source.Replace('key[0..]', 'key').Replace('key[8..]', 'key.Skip(8).ToArray()').Replace('key[16..]', 'key.Skip(16).ToArray()')
$helper = @'
public static class QrcReference {
    public static byte[] Compress(byte[] input) {
        using (var stream = new MemoryStream()) {
            stream.WriteByte(0x78); stream.WriteByte(0x9c);
            using (var deflate = new DeflateStream(stream, CompressionLevel.Optimal, true)) {
                deflate.Write(input, 0, input.Length);
            }
            uint a = 1, b = 0;
            foreach (byte value in input) { a = (a + value) % 65521; b = (b + a) % 65521; }
            uint checksum = (b << 16) | a;
            for (int i = 24; i >= 0; i -= 8) stream.WriteByte((byte)(checksum >> i));
            return stream.ToArray();
        }
    }
    public static string Encrypt(byte[] input) {
        byte[] padded = new byte[((input.Length + 7) / 8) * 8];
        Array.Copy(input, padded, input.Length);
        byte[][][] schedule = new byte[3][][];
        for (int i = 0; i < 3; i++) {
            schedule[i] = new byte[16][];
            for (int j = 0; j < 16; j++) schedule[i][j] = new byte[6];
        }
        QQMusicDecoder.DESHelper.TripleDESKeySetup(
            Encoding.ASCII.GetBytes("!@#)(*$%123ZXC!@!@#)(NHL"), schedule, QQMusicDecoder.DESHelper.ENCRYPT);
        byte[] result = new byte[padded.Length];
        for (int i = 0; i < padded.Length; i += 8) {
            byte[] block = new byte[8];
            Array.Copy(padded, i, block, 0, 8);
            byte[] encrypted = new byte[8];
            QQMusicDecoder.DESHelper.TripleDESCrypt(block, encrypted, schedule);
            Array.Copy(encrypted, 0, result, i, 8);
        }
        return BitConverter.ToString(result).Replace("-", "").ToLowerInvariant();
    }
    public static string[] Samples() {
        return new string[] {
            "<QrcInfos><LyricInfo LyricCount=\"1\"><Lyric_1 LyricType=\"1\" LyricContent=\"[ti:Fixture]\n[ar:Unit Test]\n[al:Synthetic]\n[offset:-100]\n[1000,2000]\u4f60(1000,500)\u597d(1500,500)\n[4000,1000]A(4000,400)&amp;B(4400,600)\"/></LyricInfo></QrcInfos>",
            "[ti:Unicode]\r\n[100,900]\ud83c\udfb5(100,300)e\u0301(400,600)",
            "[0,1]" + new string('x', 20000) + "(0,1)"
        };
    }
}
'@
Add-Type -TypeDefinition ("using System; using System.IO; using System.IO.Compression; using System.Linq; using System.Text;`n" + $source + $helper) -ReferencedAssemblies 'System.dll', 'System.Core.dll', 'System.IO.Compression.dll'
$samples = @()
foreach ($plain in [QrcReference]::Samples()) {
    $compressed = [QrcReference]::Compress([Text.Encoding]::UTF8.GetBytes($plain))
    $hex = [QrcReference]::Encrypt($compressed)
    $samples += [ordered]@{ plaintext = $plain; hex = $hex; api = [Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes($hex)) }
}
$badData = [ordered]@{
    invalidUtf8 = [QrcReference]::Encrypt([QrcReference]::Compress([byte[]]@(0xff, 0xfe)))
    truncatedZlib = [QrcReference]::Encrypt([byte[]]@(0x78, 0x9c, 0x4b, 0x04))
    invalidChecksum = [QrcReference]::Encrypt([byte[]]@(0x78, 0x9c, 0x4b, 0x04, 0x00, 0, 0, 0, 0))
    decompressionBomb = [QrcReference]::Encrypt([QrcReference]::Compress((New-Object byte[] (2 * 1024 * 1024 + 1))))
    nonzeroPadding = [QrcReference]::Encrypt([byte[]]@(0x78, 0x9c, 0x4b, 0x04, 0x00, 0x00, 0x62, 0x00, 0x62, 0xff))
}
$translation = "[offset:50]`n[00:01.50]$([char]0x8bd1)$([char]0x6587)A`n[00:03.00]$([char]0x8bd1)$([char]0x6587)B"
$translationHex = [QrcReference]::Encrypt([QrcReference]::Compress([Text.Encoding]::UTF8.GetBytes($translation)))
$translationVector = [ordered]@{ plaintext = $translation; hex = $translationHex; api = [Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes($translationHex)) }
$result = [ordered]@{ source = $sourceUrl; revision = $revision; samples = $samples; invalid = $badData; translation = $translationVector }
$directory = Split-Path -Parent $OutputPath
[IO.Directory]::CreateDirectory($directory) | Out-Null
[IO.File]::WriteAllText($OutputPath, ($result | ConvertTo-Json -Depth 8), (New-Object Text.UTF8Encoding($false)))
Write-Output "Generated independent QRC vectors: $OutputPath"
