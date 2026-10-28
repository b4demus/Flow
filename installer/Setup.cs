using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Drawing;
using System.IO;
using System.IO.Compression;
using System.Reflection;
using System.Security.Cryptography;
using System.Text;
using System.Threading.Tasks;
using System.Web.Script.Serialization;
using System.Windows.Forms;

internal static class Setup
{
    static readonly JavaScriptSerializer Json = new JavaScriptSerializer { MaxJsonLength = 4 * 1024 * 1024 };
    static string Quote(string value) { return "'" + value.Replace("'", "''") + "'"; }
    static string Extract()
    {
        string root = Path.Combine(Path.GetTempPath(), "Flow-Setup-" + Guid.NewGuid().ToString("N"));
        Directory.CreateDirectory(root);
        try
        {
            string zip = Path.Combine(root, "bundle.zip");
            using (Stream resource = Assembly.GetExecutingAssembly().GetManifestResourceStream("Flow.Bundle.zip"))
            using (FileStream output = File.Create(zip)) { resource.CopyTo(output); }
            using (SHA256 sha = SHA256.Create())
            using (FileStream stream = File.OpenRead(zip))
            {
                string hash = BitConverter.ToString(sha.ComputeHash(stream)).Replace("-", "").ToLowerInvariant();
                if (hash != PayloadInfo.Sha256) throw new InvalidDataException("The embedded package checksum is incorrect.");
            }
            string folder = Path.Combine(root, "package");
            Directory.CreateDirectory(folder);
            var names = new HashSet<string>(StringComparer.OrdinalIgnoreCase);
            using (ZipArchive archive = ZipFile.OpenRead(zip))
            foreach (ZipArchiveEntry entry in archive.Entries)
            {
                string path = Path.GetFullPath(Path.Combine(folder, entry.FullName.Replace('/', '\\')));
                if (!path.StartsWith(folder + Path.DirectorySeparatorChar, StringComparison.OrdinalIgnoreCase) || !names.Add(path))
                    throw new InvalidDataException("Unsafe or duplicate package path.");
                if (String.IsNullOrEmpty(entry.Name)) { Directory.CreateDirectory(path); continue; }
                Directory.CreateDirectory(Path.GetDirectoryName(path));
                entry.ExtractToFile(path);
            }
            return root;
        }
        catch { Cleanup(root); throw; }
    }
    static void Cleanup(string root)
    {
        if (String.IsNullOrEmpty(root)) return;
        try
        {
            string full = Path.GetFullPath(root), temp = Path.GetFullPath(Path.GetTempPath());
            if (!full.StartsWith(Path.Combine(temp, "Flow-Setup-"), StringComparison.OrdinalIgnoreCase) ||
                !String.Equals(Path.GetDirectoryName(full), temp.TrimEnd('\\'), StringComparison.OrdinalIgnoreCase)) return;
            if ((File.GetAttributes(full) & FileAttributes.ReparsePoint) != 0) return;
            foreach (string p in Directory.EnumerateFileSystemEntries(full, "*", SearchOption.AllDirectories))
                if ((File.GetAttributes(p) & FileAttributes.ReparsePoint) != 0) return;
            Directory.Delete(full, true);
        }
        catch { /* A locked temporary file may be removed later by Windows. */ }
    }
    static Dictionary<string, object> Run(string root, bool check)
    {
        string package = Path.Combine(root, "package"), result = Path.Combine(root, "result.json");
        string command = "& " + Quote(Path.Combine(package, "install.ps1")) + " -ResultPath " + Quote(result);
        if (check) command += " -Check -MediaCoreRoot " + Quote(Path.Combine(root, "verify-only-MediaCore"));
        command += "; exit $LASTEXITCODE";
        string ps = Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.Windows), "System32\\WindowsPowerShell\\v1.0\\powershell.exe");
        var start = new ProcessStartInfo(ps, "-NoLogo -NoProfile -ExecutionPolicy Bypass -EncodedCommand " + Convert.ToBase64String(Encoding.Unicode.GetBytes(command)))
        { UseShellExecute = false, CreateNoWindow = true, WindowStyle = ProcessWindowStyle.Hidden, WorkingDirectory = package };
        // A setup launched from PowerShell 7 can inherit its incompatible module path.
        start.EnvironmentVariables["PSModulePath"] = Path.Combine(Path.GetDirectoryName(ps), "Modules");
        int code;
        using (Process process = Process.Start(start)) { process.WaitForExit(); code = process.ExitCode; }
        if (!File.Exists(result)) throw new IOException("Installer stopped before producing a result (exit " + code + ").");
        var data = Json.Deserialize<Dictionary<string, object>>(File.ReadAllText(result, Encoding.UTF8));
        if (code != 0 || (data.ContainsKey("Status") && Convert.ToString(data["Status"]) == "Error"))
            throw new IOException(data.ContainsKey("Message") ? Convert.ToString(data["Message"]) : "Installation stopped.");
        return data;
    }
    [STAThread]
    static int Main(string[] args)
    {
        if (args.Length > 0 && args[0] == "/verify")
        {
            string root = null;
            try
            {
                root = Extract(); var data = Run(root, true);
                if (Convert.ToInt32(data["PluginCount"]) != PayloadInfo.Count) throw new InvalidDataException("Plugin count mismatch.");
                if (args.Length > 1) File.WriteAllText(args[1], Json.Serialize(new { Status = "Verified", Plugins = PayloadInfo.Count, PackageSha256 = PayloadInfo.Sha256 }), Encoding.UTF8);
                return 0;
            }
            catch (Exception e)
            {
                if (args.Length > 1) File.WriteAllText(args[1], Json.Serialize(new { Status = "Error", Message = e.Message }), Encoding.UTF8);
                return 1;
            }
            finally { Cleanup(root); }
        }
        Application.EnableVisualStyles();
        Application.SetCompatibleTextRenderingDefault(false);
        var form = new Form { Text = "Flow Setup " + PayloadInfo.Version, ClientSize = new Size(690, 500), StartPosition = FormStartPosition.CenterScreen,
            FormBorderStyle = FormBorderStyle.FixedDialog, MaximizeBox = false, Font = new Font("Segoe UI", 10), AutoScaleMode = AutoScaleMode.Dpi };
        var title = new Label { Text = "Flow для After Effects", Font = new Font("Segoe UI", 22, FontStyle.Bold), AutoSize = true, Location = new Point(24, 22) };
        var intro = new Label { Text = PayloadInfo.Count + " эффектов и " + PayloadInfo.Panels + " панели · Windows x64\r\nУстановка и обновление набора Flow", AutoSize = true, Location = new Point(27, 80) };
        var target = new TextBox { ReadOnly = true, Text = Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.ProgramFiles), "Adobe\\Common\\Plug-ins\\7.0\\MediaCore\\Flow"), Location = new Point(28, 144), Size = new Size(632, 28) };
        var info = new Label { Text = "Перед установкой сохраните проект и закройте AE.\r\nСтарые файлы сохраняются для отката, повторная установка не создаёт копий.\r\n\r\nFlow заменяет Facet, Flowtime, Dedupe и ezcurves: их старые эффекты и\r\nпанели будут убраны. Новые панели — в меню Window после перезапуска AE.", Location = new Point(27, 187), Size = new Size(636, 130) };
        var detail = new TextBox { ReadOnly = true, Multiline = true, ScrollBars = ScrollBars.Vertical, BorderStyle = BorderStyle.FixedSingle, Location = new Point(28, 318), Size = new Size(632, 104), Text = "Нажмите «Установить». Windows может запросить права администратора." };
        var bar = new ProgressBar { Location = new Point(28, 436), Size = new Size(300, 23), Style = ProgressBarStyle.Marquee, Visible = false };
        var install = new Button { Text = "Установить", Location = new Point(400, 440), Size = new Size(128, 36) };
        var close = new Button { Text = "Закрыть", Location = new Point(540, 440), Size = new Size(120, 36) };
        target.TabStop = false; detail.TabStop = false;
        form.AcceptButton = install; form.CancelButton = close;
        form.Controls.AddRange(new Control[] { title, intro, target, info, detail, bar, install, close });
        bool running = false;
        close.Click += delegate { form.Close(); };
        form.FormClosing += delegate(object sender, FormClosingEventArgs e) { if (running) e.Cancel = true; };
        install.Click += async delegate
        {
            running = true; install.Enabled = false; close.Enabled = false; bar.Visible = true;
            detail.Text = "Проверка и установка…";
            try
            {
                var data = await Task.Run(() => { string root = null; try { root = Extract(); return Run(root, false); } finally { Cleanup(root); } });
                string status = Convert.ToString(data["Status"]);
                detail.Text = status == "AlreadyInstalled" ? "Эта версия уже установлена. Файлы не изменены." :
                    "Готово. Установлено эффектов: " + data["PluginCount"] + ", панелей: " + data["PanelCount"] + " (копий AE: " + data["ScriptTargets"] + "). Убрано старых файлов и дубликатов: " + data["Duplicates"] + ".\r\nРезервная копия: " + data["Backup"] + "\r\nТеперь можно запустить After Effects.";
                install.Text = "Готово";
            }
            catch (Exception e) { detail.Text = "Установка не завершена.\r\n" + e.Message; install.Enabled = true; }
            finally { running = false; close.Enabled = true; bar.Visible = false; }
        };
        if (args.Length > 1 && args[0] == "/preview")
        {
            // Create native child handles without displaying a setup window.
            form.ShowInTaskbar = false; form.Opacity = 0;
            form.Show(); Application.DoEvents();
            using (var bitmap = new Bitmap(form.Width, form.Height))
            { form.DrawToBitmap(bitmap, new Rectangle(0, 0, form.Width, form.Height)); bitmap.Save(args[1], System.Drawing.Imaging.ImageFormat.Png); }
            form.Close();
            form.Dispose();
            return 0;
        }
        Application.Run(form);
        return 0;
    }
}
